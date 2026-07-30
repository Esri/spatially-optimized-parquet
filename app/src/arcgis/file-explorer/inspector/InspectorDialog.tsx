import {
  type CSSProperties,
  type ReactNode,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";

import { formatByteSize } from "../../../common/formatByteSize";
import type { ParquetByteCoverage } from "../../../parquet/ParquetByteCoverage";
import {
  deriveFileDetailSummary,
  type FileDetailSummary,
} from "../../../parquet/fileDetails";
import type {
  ColumnLayout,
  FileLayout,
  PageIndexLayout,
  RowGroupLayout,
} from "../../../parquet/fileLayout";
import {
  extractGeoParquetVersion,
  extractGeodisplayVersion,
  formatParquetKeyValueMetadata,
} from "../../../parquet/keyValueMetadata";
import type { ArcgisParquetPageIndexSource } from "./arcgisPageIndexes";
import {
  ParquetFileStructureStore,
  type ColumnDetailState,
  type FileStructurePage,
  type FileStructureSnapshot,
} from "./ParquetFileStructureStore";
import {
  orderByDescendingValue,
  orderByLoadedByteLength,
} from "./loadedSizeOrder";
import { assignLeaderLineLanes } from "./leaderLineLayout";
import {
  ProportionalByteBlockMap,
  type VisualByteBlock,
} from "./ProportionalByteBlockMap";
import styles from "./InspectorDialog.module.css";

export function InspectorDialog({
  snapshot,
  source,
  onClose,
}: {
  snapshot: FileStructureSnapshot;
  source: ArcgisParquetPageIndexSource;
  onClose(): void;
}) {
  const store = useMemo(
    () => new ParquetFileStructureStore(snapshot, source),
    [snapshot, source],
  );
  const state = useSyncExternalStore(store.subscribe, store.getState);
  const [downloadedOnly, setDownloadedOnly] = useState(false);
  const [orderBySize, setOrderBySize] = useState(true);
  useEffect(() => () => store.close(), [store]);
  const selectedRowGroup = state.selectedRowGroupIndex === null
    ? null
    : store.rowGroup(state.selectedRowGroupIndex);

  return (
    <calcite-dialog
      className={`${styles.dialog} calcite-mode-dark`}
      fullscreenDisabled
      modal
      open
      placement="center"
      width="l"
      oncalciteDialogClose={() => {
        store.close();
        onClose();
      }}
    >
      <div className={styles.fileStructureBreadcrumb} slot="heading">
        {selectedRowGroup ? (
          <>
            <button
              onClick={() => store.selectRowGroup(selectedRowGroup.index)}
              type="button"
            >
              File
            </button>
            <span>/ Row group {selectedRowGroup.index}</span>
          </>
        ) : (
          <span>File</span>
        )}
      </div>
      <div
        className={styles.fileStructureHeaderSwitches}
        slot="header-actions-end"
      >
        <div className={styles.fileStructureViewControl}>
          <span>Filter</span>
          <calcite-segmented-control
            aria-label="Data to show"
            scale="s"
            value={downloadedOnly ? "loaded" : "all"}
            oncalciteSegmentedControlChange={(event: Event) =>
              setDownloadedOnly(
                (
                  event.currentTarget as HTMLCalciteSegmentedControlElement
                ).value === "loaded",
              )
            }
          >
            <calcite-segmented-control-item
              checked={!downloadedOnly}
              value="all"
            >
              All
            </calcite-segmented-control-item>
            <calcite-segmented-control-item
              checked={downloadedOnly}
              value="loaded"
            >
              Loaded
            </calcite-segmented-control-item>
          </calcite-segmented-control>
        </div>
        <div className={styles.fileStructureViewControl}>
          <span>Sort</span>
          <calcite-segmented-control
            aria-label="Block order"
            scale="s"
            value={orderBySize ? "loaded-size" : "file"}
            oncalciteSegmentedControlChange={(event: Event) =>
              setOrderBySize(
                (
                  event.currentTarget as HTMLCalciteSegmentedControlElement
                ).value === "loaded-size",
              )
            }
          >
            <calcite-segmented-control-item
              checked={!orderBySize}
              value="file"
            >
              File
            </calcite-segmented-control-item>
            <calcite-segmented-control-item
              checked={orderBySize}
              value="loaded-size"
            >
              Loaded size
            </calcite-segmented-control-item>
          </calcite-segmented-control>
        </div>
      </div>
      <div className={styles.fileStructureDialogContent}>
        {selectedRowGroup ? (
          <RowGroupDetail
            coverage={snapshot.coverage}
            detailColumnId={state.detailColumnId}
            details={state.details}
            expandedColumnIds={state.expandedColumnIds}
            pageIndexes={snapshot.layout.pageIndexes}
            downloadedOnly={downloadedOnly}
            orderBySize={orderBySize}
            onToggleColumn={(column) => store.toggleColumn(column)}
            rowGroup={selectedRowGroup}
          />
        ) : (
          <FileOverview
            coverage={snapshot.coverage}
            layout={snapshot.layout}
            onSelectRowGroup={(index) => store.selectRowGroup(index)}
            downloadedOnly={downloadedOnly}
            orderBySize={orderBySize}
            selectedRowGroupIndex={state.selectedRowGroupIndex}
          />
        )}
      </div>
    </calcite-dialog>
  );
}

function FileOverview({
  coverage,
  layout,
  selectedRowGroupIndex,
  onSelectRowGroup,
  downloadedOnly,
  orderBySize,
}: {
  coverage: ParquetByteCoverage;
  layout: FileLayout;
  selectedRowGroupIndex: number | null;
  onSelectRowGroup(index: number): void;
  downloadedOnly: boolean;
  orderBySize: boolean;
}) {
  const { footer, rowGroups } = layout;
  const detailSummary = useMemo(
    () => deriveFileDetailSummary(layout),
    [layout],
  );
  const leaderSvgRef = useRef<SVGSVGElement>(null);
  const columnSegmentRefs = useRef(new Map<string, HTMLSpanElement>());
  const columnLabelRefs = useRef(new Map<string, HTMLSpanElement>());
  const [leaderLines, setLeaderLines] = useState<
    ReadonlyArray<{ fieldName: string; points: string }>
  >([]);
  const filteredRowGroups = downloadedOnly
    ? rowGroups.filter((rowGroup) =>
        rowGroup.columns.some(
          (column) => coverage.coveredByteLength(column.byteRange) > 0,
        ),
      )
    : rowGroups;
  const visibleRowGroups = orderByLoadedByteLength(
    filteredRowGroups,
    coverage,
    orderBySize,
  );
  const {
    columnOverview,
    largestColumns,
    totalColumnByteLength,
  } = useMemo(() => {
    const aggregatedColumns = Array.from(
      rowGroups.reduce((columnMap, rowGroup) => {
        for (const column of rowGroup.columns) {
          const summary = columnMap.get(column.fieldName) ?? {
            fieldName: column.fieldName,
            byteLength: 0,
            loadedByteLength: 0,
          };
          summary.byteLength += rangeLength(column.byteRange);
          summary.loadedByteLength += coverage.coveredByteLength(
            column.byteRange,
          );
          columnMap.set(column.fieldName, summary);
        }
        return columnMap;
      }, new Map<string, {
        fieldName: string;
        byteLength: number;
        loadedByteLength: number;
      }>()),
    ).map(([, column], index) => ({
      ...column,
      paletteIndex: index % 10,
    }));
    const filteredColumns = downloadedOnly
      ? aggregatedColumns.filter((column) => column.loadedByteLength > 0)
      : aggregatedColumns;
    const orderedColumns = orderByDescendingValue(
      filteredColumns,
      (column) => column.loadedByteLength,
      orderBySize,
    );
    const columns = orderedColumns.map((column) => ({
      ...column,
      byteLength: downloadedOnly
        ? column.loadedByteLength
        : column.byteLength,
    }));
    const columnOrder = new Map(
      columns.map((column, index) => [column.fieldName, index]),
    );

    return {
      columnOverview: columns,
      largestColumns: [...columns]
        .sort((first, second) => second.byteLength - first.byteLength)
        .slice(0, 5)
        .sort(
          (first, second) =>
            (columnOrder.get(first.fieldName) ?? 0) -
            (columnOrder.get(second.fieldName) ?? 0),
        ),
      totalColumnByteLength: columns.reduce(
        (total, column) => total + column.byteLength,
        0,
      ),
    };
  }, [coverage, downloadedOnly, orderBySize, rowGroups]);

  useLayoutEffect(() => {
    const svg = leaderSvgRef.current;
    if (!svg) {
      return;
    }

    const updateLeaderLines = () => {
      const svgBounds = svg.getBoundingClientRect();
      if (svgBounds.width === 0) {
        return;
      }

      const measuredLines = largestColumns.flatMap((column) => {
          const segment = columnSegmentRefs.current.get(column.fieldName);
          const label = columnLabelRefs.current.get(column.fieldName);
          if (!segment || !label) {
            return [];
          }

          const segmentBounds = segment.getBoundingClientRect();
          const labelBounds = label.getBoundingClientRect();
          const segmentCenter =
            ((segmentBounds.left + segmentBounds.width / 2 - svgBounds.left) /
              svgBounds.width) *
            100;
          const labelCenter =
            ((labelBounds.left + labelBounds.width / 2 - svgBounds.left) /
              svgBounds.width) *
            100;

          return [{
            id: column.fieldName,
            start: segmentCenter,
            end: labelCenter,
          }];
        });
      const minimumGap = (2 / svgBounds.width) * 100;
      const positionedLines = assignLeaderLineLanes(
        measuredLines,
        minimumGap,
      );
      setLeaderLines(positionedLines.map((line) => {
        const laneY = 3 + line.lane * 4;

        return {
          fieldName: line.id,
          points: `${line.start},0 ${line.start},${laneY} ${line.end},${laneY} ${line.end},20`,
        };
      }));
    };

    updateLeaderLines();

    const resizeObserver = new ResizeObserver(updateLeaderLines);
    resizeObserver.observe(svg);

    return () => resizeObserver.disconnect();
  }, [largestColumns]);

  const rowGroupBlocks: VisualByteBlock[] = visibleRowGroups.map((rowGroup) => ({
      id: `row-group-${rowGroup.index}`,
      byteRange: rowGroup.byteRange,
      className: [
        styles.rowGroup,
        selectedRowGroupIndex === rowGroup.index ? styles.selected : null,
      ].filter(Boolean).join(" "),
      onClick: () => onSelectRowGroup(rowGroup.index),
      label: (
        <>
          <strong>RG {rowGroup.index}</strong>
          <span>{formatByteSize(rangeLength(rowGroup.byteRange))}</span>
        </>
      ),
    }));

  return (
    <section className={styles.fileStructureOverview}>
      <FileDetails layout={layout} summary={detailSummary} />
      <div className={styles.fileStructureFileColumns}>
        <div className={styles.fileStructureColumnChart}>
          <div className={styles.fileStructureColumnShareBar}>
            {columnOverview.map((column) => {
            const filePercent =
              totalColumnByteLength === 0
                ? 0
                : (column.byteLength / totalColumnByteLength) * 100;
              return (
                <ColumnOverviewSegment
                  filePercent={filePercent}
                  fieldName={column.fieldName}
                  loadedOnly={downloadedOnly}
                  paletteIndex={column.paletteIndex}
                  key={column.fieldName}
                  byteLength={column.byteLength}
                  elementRef={(element) => {
                    if (element) {
                      columnSegmentRefs.current.set(column.fieldName, element);
                    } else {
                      columnSegmentRefs.current.delete(column.fieldName);
                    }
                  }}
                />
              );
            })}
          </div>
          <svg
            aria-hidden="true"
            className={styles.fileStructureColumnLeaders}
            preserveAspectRatio="none"
            ref={leaderSvgRef}
            viewBox="0 0 100 20"
          >
            {leaderLines.map((line) => (
              <polyline
                fill="none"
                key={line.fieldName}
                points={line.points}
              />
            ))}
          </svg>
          <div className={styles.fileStructureColumnLeaderLabels}>
            {largestColumns.map((column) => (
              <span
                key={column.fieldName}
                ref={(element) => {
                  if (element) {
                    columnLabelRefs.current.set(column.fieldName, element);
                  } else {
                    columnLabelRefs.current.delete(column.fieldName);
                  }
                }}
              >
                <strong>{column.fieldName}</strong>
                <small>{formatByteSize(column.byteLength)}</small>
              </span>
            ))}
          </div>
        </div>
      </div>
      <div className={styles.fileStructureRowGroups}>
        <h4>Row Groups</h4>
        <div className={styles.fileStructureRowGroupPicker}>
          <ProportionalByteBlockMap
            ariaLabel="Parquet row groups"
            blocks={rowGroupBlocks}
            coverage={coverage}
          />
        </div>
      </div>
      <div className={styles.fileStructureFooter}>
        <h4>Footer</h4>
        <ProportionalByteBlockMap
          ariaLabel="Parquet footer"
          blocks={[{
            id: "footer",
            byteRange: footer,
            className: `${styles.metadata} ${styles.footer}`,
            label: <span>{formatByteSize(rangeLength(footer))}</span>,
          }]}
          coverage={coverage}
        />
      </div>
      <div className={styles.fileStructureFooterGuidance}>
        <span>Click a row group to inspect its columns.</span>
        <DownloadStateLegend />
      </div>
    </section>
  );
}

function FileDetails({
  layout,
  summary,
}: {
  layout: FileLayout;
  summary: FileDetailSummary;
}) {
  const [metadataButton, setMetadataButton] =
    useState<HTMLCalciteButtonElement | null>(null);
  const [metadataOpen, setMetadataOpen] = useState(false);
  const compressionCodecs = summary.compressionCodecs
    .map((codec) => codec.toUpperCase());
  const compressionName = compressionCodecs.length === 1
    ? compressionCodecs[0]
    : compressionCodecs.length > 1
      ? "Mixed"
      : "Unavailable";
  const compressionRatio = summary.compressedSize > 0
    ? `${(summary.uncompressedSize / summary.compressedSize).toFixed(1)}×`
    : "Unavailable";
  const fileName = formatFileName(layout.fileName);
  const geodisplayVersion =
    extractGeodisplayVersion(layout.keyValueMetadata) ?? "—";
  const geoParquetVersion =
    extractGeoParquetVersion(layout.keyValueMetadata) ?? "—";

  return (
    <div className={styles.fileStructureFileDetails}>
      <div className={styles.fileStructureFileName}>
        <dl>
          <FileDetail label="File name" title={layout.fileName} value={fileName} />
        </dl>
      </div>
      <div className={styles.fileStructureFileDetailFields}>
        <dl>
          <FileDetail label="Rows" value={formatCompactCount(summary.rowCount)} />
          <FileDetail label="Columns" value={summary.columnCount.toLocaleString()} />
          <FileDetail
            label="Size"
            value={formatByteSize(layout.byteLength)}
          />
          <FileDetail
            label="Compression"
            title={compressionCodecs.join(", ")}
            value={
              <>
                {compressionName}
                <small className={styles.compressionRatio}>
                  {compressionRatio.replace("×", "x")}
                </small>
              </>
            }
          />
          <div
            className={styles.fileStructureDetailDivider}
            aria-hidden="true"
          />
          <FileDetail label="SOP" value={geodisplayVersion} />
          <FileDetail label="GeoParquet" value={geoParquetVersion} />
        </dl>
      </div>
      <calcite-button
        ref={setMetadataButton}
        appearance="outline"
        className={styles.fileStructureKeysButton}
        iconStart="magnifying-glass"
        kind="neutral"
        label="Metadata"
        scale="m"
        onClick={() => {
          requestAnimationFrame(() => setMetadataOpen((open) => !open));
        }}
      >
        Keys
      </calcite-button>
      {metadataButton ? (
        <calcite-popover
          className={styles.fileStructureMetadataPopover}
          label="Parquet key-value metadata"
          open={metadataOpen}
          overlayPositioning="fixed"
          placement="bottom-end"
          referenceElement={metadataButton}
          oncalcitePopoverClose={() => setMetadataOpen(false)}
        >
          <pre>{formatParquetKeyValueMetadata(layout.keyValueMetadata)}</pre>
        </calcite-popover>
      ) : null}
    </div>
  );
}

function formatFileName(value: string): string {
  const path = value.split(/[?#]/, 1)[0].replaceAll("\\", "/");
  return path.slice(path.lastIndexOf("/") + 1) || value;
}

function formatCompactCount(count: number): string {
  return new Intl.NumberFormat(undefined, {
    maximumFractionDigits: 1,
    notation: "compact",
  }).format(count);
}

function FileDetail({
  label,
  title,
  value,
}: {
  label: string;
  title?: string;
  value: ReactNode;
}) {
  return (
    <div>
      <dt>{label}</dt>
      <dd title={title}>{value}</dd>
    </div>
  );
}

function ColumnOverviewSegment({
  byteLength,
  elementRef,
  fieldName,
  filePercent,
  loadedOnly,
  paletteIndex,
}: {
  byteLength: number;
  elementRef(element: HTMLSpanElement | null): void;
  fieldName: string;
  filePercent: number;
  loadedOnly: boolean;
  paletteIndex: number;
}) {
  const targetId = useId().replaceAll(":", "");
  const [tooltipOpen, setTooltipOpen] = useState(false);
  return (
    <>
      <span
        className={`${styles.fileStructureColumnShare} ${styles[`palette${paletteIndex}`]}`}
        id={targetId}
        onMouseEnter={() => setTooltipOpen(true)}
        onMouseLeave={() => setTooltipOpen(false)}
        ref={elementRef}
        style={{
          "--column-share-weight": Math.max(byteLength, 1),
        } as CSSProperties}
      />
      <calcite-tooltip
        className={styles.fileStructureTooltip}
        open={tooltipOpen}
        overlayPositioning="fixed"
        referenceElement={targetId}
      >
        <strong>{fieldName}</strong>
        <br />
        {formatByteSize(byteLength)} · {filePercent.toFixed(2)}% of{" "}
        {loadedOnly ? "loaded column bytes" : "column bytes"}
      </calcite-tooltip>
    </>
  );
}

function RowGroupDetail({
  coverage,
  detailColumnId,
  details,
  expandedColumnIds,
  pageIndexes,
  rowGroup,
  onToggleColumn,
  downloadedOnly,
  orderBySize,
}: {
  coverage: ParquetByteCoverage;
  detailColumnId: string | null;
  details: ReadonlyMap<string, ColumnDetailState>;
  expandedColumnIds: ReadonlySet<string>;
  pageIndexes: readonly PageIndexLayout[];
  rowGroup: RowGroupLayout;
  onToggleColumn(column: ColumnLayout): void;
  downloadedOnly: boolean;
  orderBySize: boolean;
}) {
  const detailColumn = rowGroup.columns.find(
    (column) => column.id === detailColumnId,
  );
  const filteredColumns = downloadedOnly
    ? rowGroup.columns.filter(
        (column) => coverage.coveredByteLength(column.byteRange) > 0,
      )
    : rowGroup.columns;
  const visibleColumns = orderByLoadedByteLength(
    filteredColumns,
    coverage,
    orderBySize,
  );

  return (
    <section className={styles.fileStructureRowGroup}>
      <div className={styles.fileStructureSelectedColumn}>
        <div className={styles.fileStructureColumnDetailsHeading}>
          <h4>Column Details</h4>
          <span className={detailColumn ? undefined : styles.empty}>
            {detailColumn?.fieldName ?? "No column selected"}
          </span>
        </div>
        {detailColumn ? (
          <ColumnDetail
            column={detailColumn}
            coverage={coverage}
            detail={details.get(detailColumn.id) ?? { type: "idle" }}
            pageIndexes={pageIndexes.filter(
              (index) =>
                index.rowGroupIndex === detailColumn.rowGroupIndex &&
                index.fieldName === detailColumn.fieldName,
            )}
          />
        ) : (
          <div className={styles.fileStructureEmptyDetail}>
            Select a column for more details.
          </div>
        )}
      </div>
      <div className={styles.fileStructureColumns}>
        <h4>Columns</h4>
        <div className={styles.fileStructureColumnPicker}>
          <ProportionalByteBlockMap
            ariaLabel={`Row group ${rowGroup.index} columns`}
            coverage={coverage}
            blocks={visibleColumns.map((column) => ({
              id: column.id,
              byteRange: column.byteRange,
              className: [
                styles.column,
                expandedColumnIds.has(column.id) ? styles.selected : null,
              ].filter(Boolean).join(" "),
              onClick: () => onToggleColumn(column),
              label: (
                <>
                  <strong title={column.fieldName}>
                    {formatColumnLeafName(column.fieldName)}
                  </strong>
                  <span>{formatByteSize(rangeLength(column.byteRange))}</span>
                </>
              ),
            }))}
          />
        </div>
      </div>
      <div className={styles.fileStructureFooterGuidance}>
        <span>Click a column to inspect individual pages.</span>
        <DownloadStateLegend />
      </div>
    </section>
  );
}

function formatColumnLeafName(fieldName: string): string {
  return fieldName.split(".").at(-1) ?? fieldName;
}

function DownloadStateLegend() {
  return (
    <div
      className={styles.fileStructureDownloadLegend}
      aria-label="Download state"
    >
      <span>
        <i className={styles.loaded} aria-hidden="true" />
        Loaded
      </span>
      <span>
        <i aria-hidden="true" />
        Not loaded
      </span>
    </div>
  );
}

function ColumnDetail({
  column,
  coverage,
  detail,
  pageIndexes,
}: {
  column: ColumnLayout;
  coverage: ParquetByteCoverage;
  detail: ColumnDetailState;
  pageIndexes: readonly PageIndexLayout[];
}) {
  if (detail.type === "loading" || detail.type === "idle") {
    return <DelayedColumnLoading />;
  }

  function ColumnFact({
    label,
    value,
    title = value,
  }: {
    label: string;
    value: string;
    title?: string;
  }) {
    const tooltipTargetId = useId().replaceAll(":", "");
    const tooltipContent = title !== value ? title : `${label}: ${value}`;
    return (
      <div id={tooltipTargetId}>
        <span>{label}:</span>
        <strong>{value}</strong>
        <calcite-tooltip
          className={styles.fileStructureTooltip}
          overlayPositioning="fixed"
          referenceElement={tooltipTargetId}
        >
          {tooltipContent}
        </calcite-tooltip>
      </div>
    );
  }

  function formatOptionalValue(value: number | string | null): string {
    return value === null ? "—" : String(value);
  }

  function formatCompressionRatio(column: ColumnLayout): string {
    if (column.compressedSize === 0) {
      return "—";
    }
    return `${(column.uncompressedSize / column.compressedSize).toFixed(2)}×`;
  }

  function DelayedColumnLoading() {
    const [visible, setVisible] = useState(false);

    useEffect(() => {
      const timeout = setTimeout(() => setVisible(true), 500);
      return () => clearTimeout(timeout);
    }, []);

    return (
      <div className={styles.fileStructureEmptyDetail}>
        {visible ? (
          <>
            <span className={styles.fileStructureSpinner} aria-hidden="true" />
            <span>Loading column details…</span>
          </>
        ) : null}
      </div>
    );
  }
  if (detail.type === "failed") {
    return (
      <div className={`${styles.fileStructureStatus} ${styles.error}`}>
        {detail.message}
      </div>
    );
  }
  if (detail.type === "unavailable") {
    return (
      <div className={styles.fileStructureStatus}>
        Offset index unavailable. Page boundaries cannot be expanded.
      </div>
    );
  }

  const pageBlocks: VisualByteBlock[] = detail.pages.map((page) => ({
      id: `${column.id}-page-${page.pageIndex}`,
      elementId: `${column.id}-page-${page.pageIndex}`,
      byteRange: page.byteRange,
      className: styles.page,
      label: (
        <>
          <strong>Page {page.pageIndex}</strong>
          <span>{formatByteSize(page.compressedPageSize)}</span>
        </>
      ),
    }));
  const blocks: VisualByteBlock[] = [
    ...pageBlocks,
    ...detail.gaps.map((gap, index) => ({
      id: `${column.id}-gap-${index}`,
      byteRange: gap.byteRange,
      className: styles.gap,
      label: (
        <>
          <strong>Dict</strong>
          <span>{formatByteSize(rangeLength(gap.byteRange))}</span>
        </>
      ),
    })),
  ].sort((first, second) => first.byteRange.start - second.byteRange.start);

  return (
    <div className={styles.fileStructureColumnDetail}>
      <div className={styles.fileStructureColumnVisualArea}>
        <span className={styles.fileStructureColumnSize}>
          <span>{formatByteSize(rangeLength(column.byteRange))}</span>
        </span>
        <div className={styles.fileStructureColumnVisuals}>
          {pageIndexes.length > 0 ? (
            <div className={styles.fileStructureMetadataGroup}>
              <div className={styles.fileStructureIndexBlocks}>
                <ProportionalByteBlockMap
                  ariaLabel={`${column.fieldName} index byte ranges`}
                  blocks={pageIndexes.map((index) => ({
                    id: index.id,
                    byteRange: index.byteRange,
                    className: styles.metadata,
                    label: (
                      <>
                        <strong>{index.kind === "column" ? "Column" : "Offset"}</strong>
                        <span>{formatByteSize(rangeLength(index.byteRange))}</span>
                      </>
                    ),
                  }))}
                  coverage={coverage}
                />
              </div>
              <span>Page Index</span>
            </div>
          ) : null}
          <div className={styles.fileStructurePageScroll}>
            <ProportionalByteBlockMap
              ariaLabel={`${column.fieldName} pages`}
              blocks={blocks}
              coverage={coverage}
            />
            {detail.pages.map((page) => (
              <calcite-tooltip
                className={styles.fileStructureTooltip}
                key={`${column.id}-page-${page.pageIndex}-tooltip`}
                overlayPositioning="fixed"
                referenceElement={`${column.id}-page-${page.pageIndex}`}
              >
                <strong>Page {page.pageIndex}</strong>
                <br />
                Bytes {page.byteRange.start.toLocaleString()}–
                {(page.byteRange.end - 1).toLocaleString()}
                <br />
                Rows {page.rowStart.toLocaleString()}–
                {(page.rowEnd - 1).toLocaleString()}
                <br />
                Compressed size {formatByteSize(page.compressedPageSize)}
                <br />
                {formatPageStatistic(page.statistic)}
              </calcite-tooltip>
            ))}
          </div>
        </div>
      </div>
      <div className={styles.fileStructureColumnFacts}>
        <ColumnFact label="Type" value={column.logicalType ?? column.physicalType} />
        <ColumnFact label="Physical" value={column.physicalType} />
        <ColumnFact
          label="Compression"
          value={`${column.compression} (${formatCompressionRatio(column)})`}
        />
        <ColumnFact
          label="Def Levels"
          value={`${column.maxDefinitionLevel} (Nullable ${
            column.nullable ? "✓" : "✕"
          })`}
          title="Maximum definition level: the optional nesting depth used to represent null and missing values."
        />
        <ColumnFact label="Nulls" value={formatOptionalValue(column.nullCount)} />
        <ColumnFact
          label="Rep Levels"
          value={column.maxRepetitionLevel.toString()}
          title="Maximum repetition level: the repeated nesting depth used to represent lists and repeated values."
        />
        <ColumnFact
          label="Encodings"
          value={column.encodings.length.toString()}
          title={column.encodings.join(", ")}
        />
        <ColumnFact
          label="Range"
          value={`${formatOptionalValue(column.minimumValue)}–${formatOptionalValue(
            column.maximumValue,
          )}`}
        />
      </div>
    </div>
  );
}

function formatPageStatistic(
  statistic: FileStructurePage["statistic"],
): string {
  switch (statistic.type) {
    case "bounds":
      return `Values ${statistic.min.value}–${statistic.max.value}`;
    case "nullOnly":
      return "Values — · Null values only";
    case "unknown":
      return "Values unavailable";
  }
}

function rangeLength(range: { start: number; end: number }): number {
  return range.end - range.start;
}

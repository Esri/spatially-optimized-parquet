import {
  memo,
  type CSSProperties,
  type RefObject,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import { createPortal } from "react-dom";

import type { Dataset } from "../../common/dataset/datasets";
import type { ArcgisDatasetSessionResult } from "../useArcgisDatasetSession";
import {
  resolveParquetPageIndexSource,
  type ParquetPageIndexSource,
} from "./inspector/parquetPageIndexes";
import { formatByteSize } from "../../common/formatByteSize";
import { formatInteger, formatPercent } from "../../common/formatNumber";
import type {
  DownloadBlockLayout,
  DownloadTrackLayout,
} from "../../parquet/displayLayout";
import { formatParquetFileName } from "../../parquet/formatFileName";
import { InspectorDialog } from "./inspector/InspectorDialog";
import { Minimap } from "./minimap/Minimap";
import {
  type DownloadColumnStatistics,
  type DownloadRowGroupItemCoverage,
  type DownloadRowGroupCoverage,
  type DownloadSessionView,
} from "./download/ParquetDownloadSession";
import styles from "./FileExplorer.module.css";

export interface FileExplorerProps {
  dataset: Dataset;
  layout:
    | { type: "desktop" }
    | { type: "compact"; visible: boolean };
  mapElementRef: RefObject<HTMLArcgisMapElement | null>;
  session: ArcgisDatasetSessionResult;
}

const columnThresholdSliderMaximum = 1000;
const minimumPositiveColumnThreshold = 1024;
const defaultColumnDownloadThreshold = 250 * 1024;
const tooltipOpenDelayMs = 100;
const tooltipGracePeriodMs = 200;

/**
 * Presents live Parquet download activity as file tracks, row-group details, and inspection tools.
 * It coordinates the session's external stores with responsive explorer state so high-frequency byte updates remain localized.
 */
export const FileExplorer = memo(function FileExplorer({
  dataset,
  layout,
  mapElementRef,
  session,
}: FileExplorerProps) {
  const {
    download: datasetDownload,
    parquetSource,
  } = session;
  const datasetId = dataset.id;
  const [fileSelection, setFileSelection] = useState({
    datasetId,
    index: 0,
  });
  const selectedFileIndex = fileSelection.datasetId === datasetId
    ? Math.min(fileSelection.index, Math.max(0, datasetDownload.files.length - 1))
    : 0;
  const selectedFile = datasetDownload.files[selectedFileIndex] ?? null;
  const downloadSession = datasetDownload.aggregateDownload;
  const fileStructureSnapshot = useMemo(
    () => selectedFile?.download.createFileStructureSnapshot() ?? null,
    [selectedFile],
  );
  const [fileStructureDialog, setFileStructureDialog] = useState<{
    datasetId: string;
    source: ParquetPageIndexSource;
  } | null>(null);
  const openFileStructure = () => {
    if (!fileStructureSnapshot || !parquetSource) {
      return;
    }
    setFileStructureDialog({
      datasetId,
      source: resolveParquetPageIndexSource(parquetSource),
    });
  };
  const selectFile = (index: number) => {
    setFileSelection({ datasetId, index });
  };
  const selectPreviousFile = () => {
    selectFile(Math.max(0, selectedFileIndex - 1));
  };
  const selectNextFile = () => {
    selectFile(
      Math.min(datasetDownload.files.length - 1, selectedFileIndex + 1),
    );
  };
  const content = (
    <>
      <div className={styles.fileExplorerOverview}>
        <Minimap
          boundsApproximate={datasetDownload.boundsApproximate}
          bounds={datasetDownload.approximateBounds}
          dataset={dataset}
          diagnosticsReady={datasetDownload.files.length > 0}
          fullExtent={
            dataset.kind === "portal-item"
              ? session.layer?.fullExtent ?? null
              : null
          }
          key={datasetId}
          mainMapElementRef={mapElementRef}
        />
        <div className={styles.fileStructureAction}>
          <calcite-button
            appearance="solid"
            className={styles.fileStructureButton}
            iconStart="magnifying-glass"
            onClick={openFileStructure}
            width="full"
          >
            Explore file layout
          </calcite-button>
        </div>
      </div>
      <FileDownload downloadSession={downloadSession} />
    </>
  );

  return (
    <>
      {layout.type === "desktop" ? (
        <div className={styles.gridDetailsColumn}>{content}</div>
      ) : layout.visible ? (
        <aside aria-label="Details" className={styles.responsiveDetailsOverlay}>
          {content}
        </aside>
      ) : null}
      {fileStructureDialog?.datasetId === datasetId &&
      fileStructureSnapshot &&
      selectedFile ? (
        createPortal(
          <InspectorDialog
            fileCount={datasetDownload.files.length}
            fileIndex={selectedFileIndex}
            onNextFile={selectNextFile}
            onClose={() => setFileStructureDialog(null)}
            onPreviousFile={selectPreviousFile}
            snapshot={fileStructureSnapshot}
            source={fileStructureDialog.source}
          />,
          document.body,
        )
      ) : null}
    </>
  );
});

const FileDownload = memo(function FileDownload({
  downloadSession,
}: {
  downloadSession: DownloadSessionView;
}) {
  const topology = useSyncExternalStore(
    downloadSession.topology.subscribe,
    downloadSession.topology.getSnapshot,
  );
  const [minimumDownloadedByteLength, setMinimumDownloadedByteLength] = useState(
    defaultColumnDownloadThreshold,
  );
  const maximumColumnByteLength = Math.max(
    minimumPositiveColumnThreshold,
    ...topology.tracks
      .filter((track) => track.kind === "column")
      .map((track) => track.byteLength),
  );
  return (
    <>
      {topology.layout ? (
        <div className={styles.columnThresholdFrame}>
          <BytesLoaded
            byteLength={topology.layout.byteLength}
            downloadSession={downloadSession}
          />
          <ColumnDownloadThreshold
            maximumByteLength={maximumColumnByteLength}
            minimumDownloadedByteLength={minimumDownloadedByteLength}
            onChange={setMinimumDownloadedByteLength}
          />
        </div>
      ) : null}
      <div className={styles.occupancyGridFrame}>
        <div
          className={styles.occupancyFlow}
          role="img"
          aria-label="Parquet download coverage by track"
        >
          {topology.layout ? (
            <DownloadTrackList
              downloadSession={downloadSession}
              minimumDownloadedByteLength={minimumDownloadedByteLength}
              tracks={topology.tracks}
            />
          ) : (
            <span className={styles.occupancyStatus}>
              {topology.error
                ? "Unable to load Parquet diagnostics."
                : null}
            </span>
          )}
        </div>
      </div>
      {topology.layout ? (
        <DownloadCoverageFooter
          downloadSession={downloadSession}
          minimumDownloadedByteLength={minimumDownloadedByteLength}
          tracks={topology.tracks}
        />
      ) : null}
    </>
  );
});

const BytesLoaded = memo(function BytesLoaded({
  downloadSession,
  byteLength,
}: {
  downloadSession: DownloadSessionView;
  byteLength: number | null;
}) {
  const summary = useSyncExternalStore(
    downloadSession.summary.subscribe,
    downloadSession.summary.getSnapshot,
  );
  return (
    <div className={styles.bytesLoaded}>
      <div className={styles.fileStatLabel}>Bytes Loaded</div>
      <div className={styles.fileStatValue}>
        {formatByteSize(summary.downloadedByteLength)}
        <span className={styles.fileStatUnit}>
          / {byteLength === null ? "…" : formatByteSize(byteLength)}
        </span>
      </div>
    </div>
  );
});

const ColumnDownloadThreshold = memo(function ColumnDownloadThreshold({
  maximumByteLength,
  minimumDownloadedByteLength,
  onChange,
}: {
  maximumByteLength: number;
  minimumDownloadedByteLength: number;
  onChange: (byteLength: number) => void;
}) {
  const sliderValue = byteLengthToThresholdSliderValue(
    minimumDownloadedByteLength,
    maximumByteLength,
  );

  return (
    <label className={styles.columnThresholdControl}>
      <span>
        Show with at least
        <strong>{formatByteSize(minimumDownloadedByteLength)}</strong>
      </span>
      <calcite-slider
        label="Minimum downloaded bytes required to show a column"
        max={columnThresholdSliderMaximum}
        min={0}
        oncalciteSliderInput={(event: Event) => {
          const value = Number((event.currentTarget as HTMLCalciteSliderElement).value);
          onChange(thresholdSliderValueToByteLength(value, maximumByteLength));
        }}
        step={1}
        value={sliderValue}
      />
    </label>
  );
});

const DownloadTrackList = memo(function DownloadTrackList({
  downloadSession,
  minimumDownloadedByteLength,
  tracks,
}: {
  downloadSession: DownloadSessionView;
  minimumDownloadedByteLength: number;
  tracks: readonly DownloadTrackLayout[];
}) {
  const [activeTooltipTrackId, setActiveTooltipTrackId] = useState<string | null>(null);
  const summary = useSyncExternalStore(
    downloadSession.summary.subscribe,
    downloadSession.summary.getSnapshot,
  );
  const trackOrder = new Map(tracks.map((track, index) => [track.id, index]));
  const sortedTracks = [...tracks].sort((first, second) => {
    const downloadedByteDifference =
      downloadSession.track(second.id).getSnapshot().downloadedByteLength -
      downloadSession.track(first.id).getSnapshot().downloadedByteLength;
    return downloadedByteDifference !== 0
      ? downloadedByteDifference
      : (trackOrder.get(first.id) ?? 0) - (trackOrder.get(second.id) ?? 0);
  });

  return sortedTracks.map((track) => (
    <DownloadTrack
      downloadSession={downloadSession}
      key={track.id}
      minimumDownloadedByteLength={minimumDownloadedByteLength}
      onTooltipClose={() => {
        setActiveTooltipTrackId((activeTrackId) =>
          activeTrackId === track.id ? null : activeTrackId,
        );
      }}
      onTooltipOpen={() => setActiveTooltipTrackId(track.id)}
      tooltipActive={activeTooltipTrackId === track.id}
      totalDownloadedByteLength={summary.downloadedByteLength}
      track={track}
    />
  ));
});

const DownloadCoverageFooter = memo(function DownloadCoverageFooter({
  downloadSession,
  minimumDownloadedByteLength,
  tracks,
}: {
  downloadSession: DownloadSessionView;
  minimumDownloadedByteLength: number;
  tracks: readonly DownloadTrackLayout[];
}) {
  const summary = useSyncExternalStore(
    downloadSession.summary.subscribe,
    downloadSession.summary.getSnapshot,
  );
  const visibleColumnCount = tracks.filter((track) => {
    if (track.kind !== "column") {
      return false;
    }
    const snapshot = downloadSession.track(track.id).getSnapshot();
    return (
      snapshot.visibleBlockCount > 0 &&
      snapshot.downloadedByteLength >= minimumDownloadedByteLength
    );
  }).length;

  return (
    <div className={styles.occupancyFooter}>
      <span className={styles.occupancySummary}>
        {visibleColumnCount}/{summary.columnCount} columns
      </span>
      <span className={styles.occupancyLegend} aria-label="Download state">
        <span>
          <i className={styles.loaded} aria-hidden="true" />
          Loaded
        </span>
        <span>
          <i aria-hidden="true" />
          Not loaded
        </span>
      </span>
    </div>
  );
});

const DownloadTrack = memo(function DownloadTrack({
  track,
  downloadSession,
  minimumDownloadedByteLength,
  onTooltipClose,
  onTooltipOpen,
  tooltipActive,
  totalDownloadedByteLength,
}: {
  track: DownloadTrackLayout;
  downloadSession: DownloadSessionView;
  minimumDownloadedByteLength: number;
  onTooltipClose: () => void;
  onTooltipOpen: () => void;
  tooltipActive: boolean;
  totalDownloadedByteLength: number;
}) {
  const channel = useMemo(
    () => downloadSession.track(track.id),
    [downloadSession, track.id],
  );
  const snapshot = useSyncExternalStore(channel.subscribe, channel.getSnapshot);
  const tooltip = useDownloadTrackTooltip({
    onClose: onTooltipClose,
    onOpen: onTooltipOpen,
    tooltipActive,
  });
  const blockById = useMemo(
    () => new Map(track.blocks.map((block) => [block.id, block])),
    [track.blocks],
  );
  const supportsRowGroupTooltip =
    track.kind === "column" || track.kind === "page-index";

  if (
    snapshot.visibleBlockCount === 0 ||
    (track.kind === "column" &&
      snapshot.downloadedByteLength < minimumDownloadedByteLength)
  ) {
    return null;
  }

  const rowGroupCoverage = tooltip.hoveredBlock && tooltipActive
    ? downloadSession.rowGroupCoverage(track.id)
    : [];
  const highlightedBlockMasks =
    tooltip.hoveredRowGroupIndex === null || !tooltipActive
    ? undefined
    : downloadSession.rowGroupBlockMasks(
        track.id,
        tooltip.hoveredRowGroupIndex,
      );

  return (
    <span
      className={[
        styles.columnRange,
        tooltip.hoveredBlock && tooltipActive
          ? styles.columnRangeSelected
          : null,
      ].filter(Boolean).join(" ")}
      onMouseLeave={tooltip.scheduleClose}
    >
      <span className={styles.columnRangeContent}>
        <span className={styles.columnLabel}>
          <span className={styles.columnDownloadShare}>
            {formatDownloadPercent(
              totalDownloadedByteLength === 0
                ? 0
                : (snapshot.downloadedByteLength / totalDownloadedByteLength) *
                    100,
            )}
          </span>
          {track.label}
          <DownloadTrackProgress
            downloadedByteLength={snapshot.downloadedByteLength}
            track={track}
          />
        </span>
        <span className={styles.columnBlocks}>
          {snapshot.visibleBlockIds.map((blockId) => {
            const block = blockById.get(blockId);
            return block ? (
              <DownloadChunk
                block={block}
                downloadSession={downloadSession}
                highlightedMask={highlightedBlockMasks?.get(block.id) ?? 0}
                key={block.id}
                onHover={
                  supportsRowGroupTooltip ? tooltip.scheduleOpen : undefined
                }
                onHoverEnd={
                  supportsRowGroupTooltip ? tooltip.cancelOpen : undefined
                }
              />
            ) : null;
          })}
        </span>
      </span>
      {tooltip.hoveredBlock && tooltipActive && supportsRowGroupTooltip ? (
        createPortal(
          <TrackRowGroupTooltip
            anchor={tooltip.hoveredBlock}
            coverage={rowGroupCoverage}
            onMouseEnter={tooltip.cancelClose}
            onMouseLeave={tooltip.scheduleClose}
            onRowGroupHover={tooltip.setHoveredRowGroupIndex}
            title={`${track.label} by row group`}
          />,
          document.body,
        )
      ) : null}
    </span>
  );
});

const DownloadTrackProgress = memo(function DownloadTrackProgress({
  track,
  downloadedByteLength,
}: {
  track: DownloadTrackLayout;
  downloadedByteLength: number;
}) {
  return (
    <span className={styles.columnProgress}>
      ({formatByteSize(downloadedByteLength)}/{formatByteSize(track.byteLength)})
    </span>
  );
});

const DownloadChunk = memo(function DownloadChunk({
  block,
  downloadSession,
  highlightedMask = 0,
  onHover,
  onHoverEnd,
}: {
  block: DownloadBlockLayout;
  downloadSession: DownloadSessionView;
  highlightedMask?: number;
  onHover?: (element: HTMLSpanElement) => void;
  onHoverEnd?: () => void;
}) {
  const channel = useMemo(
    () => downloadSession.block(block.id),
    [block.id, downloadSession],
  );
  const snapshot = useSyncExternalStore(channel.subscribe, channel.getSnapshot);
  const hasVisibleCoverage =
    snapshot.cachedMask !== 0 || snapshot.loadingMask !== 0 || snapshot.activeMask !== 0;

  if (!hasVisibleCoverage) {
    return null;
  }

  return (
    <span
      aria-label={`Download block with ${block.subparts.length} byte subparts`}
      className={styles.chunk}
      onMouseEnter={onHover ? (event) => onHover(event.currentTarget) : undefined}
      onMouseLeave={onHoverEnd}
    >
      {block.subparts.map((subpart) => (
        <DownloadSubpart
          activeMask={snapshot.activeMask}
          cachedMask={snapshot.cachedMask}
          highlighted={(highlightedMask & (1 << subpart.index)) !== 0}
          index={subpart.index}
          key={subpart.index}
          loadingMask={snapshot.loadingMask}
        />
      ))}
    </span>
  );
});

const DownloadSubpart = memo(function DownloadSubpart({
  index,
  cachedMask,
  loadingMask,
  activeMask,
  highlighted,
}: {
  index: number;
  cachedMask: number;
  loadingMask: number;
  activeMask: number;
  highlighted: boolean;
}) {
  const mask = 1 << index;
  const state = (activeMask & mask) !== 0
    ? "active"
    : (loadingMask & mask) !== 0
      ? "loading"
      : (cachedMask & mask) !== 0
        ? "cached"
        : "empty";
  const stateClassName = state === "empty" ? null : styles[state];
  return (
    <span
      className={[
        styles.chunkSubpart,
        stateClassName,
        highlighted ? styles.selected : null,
      ].filter(Boolean).join(" ")}
    />
  );
});

const TrackRowGroupTooltip = memo(function TrackRowGroupTooltip({
  anchor,
  coverage,
  onMouseEnter,
  onMouseLeave,
  onRowGroupHover,
  title,
}: {
  anchor: HTMLSpanElement;
  coverage: readonly DownloadRowGroupCoverage[];
  onMouseEnter: () => void;
  onMouseLeave: () => void;
  onRowGroupHover: (rowGroupIndex: number | null) => void;
  title: string;
}) {
  const tooltipRef = useRef<HTMLSpanElement>(null);
  const [tooltipStyle, setTooltipStyle] = useState<CSSProperties>({
    visibility: "hidden",
  });
  const [hoveredRowGroup, setHoveredRowGroup] = useState<{
    element: HTMLSpanElement;
    rowGroupIndex: number;
  } | null>(null);
  const closeRowGroupTooltipTimerRef =
    useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => () => {
    if (closeRowGroupTooltipTimerRef.current !== null) {
      clearTimeout(closeRowGroupTooltipTimerRef.current);
    }
  }, []);

  const cancelRowGroupTooltipClose = () => {
    if (closeRowGroupTooltipTimerRef.current !== null) {
      clearTimeout(closeRowGroupTooltipTimerRef.current);
      closeRowGroupTooltipTimerRef.current = null;
    }
  };
  const keepTooltipHierarchyOpen = () => {
    cancelRowGroupTooltipClose();
    onMouseEnter();
  };
  const scheduleRowGroupTooltipClose = () => {
    cancelRowGroupTooltipClose();
    closeRowGroupTooltipTimerRef.current = setTimeout(() => {
      closeRowGroupTooltipTimerRef.current = null;
      setHoveredRowGroup(null);
      onRowGroupHover(null);
    }, tooltipGracePeriodMs);
  };

  useLayoutEffect(() => {
    const tooltip = tooltipRef.current;
    if (!tooltip) {
      return;
    }

    const updatePosition = () => {
      setTooltipStyle(createColumnTooltipStyle(anchor, tooltip));
    };
    updatePosition();

    const resizeObserver = new ResizeObserver(updatePosition);
    resizeObserver.observe(tooltip);
    window.addEventListener("resize", updatePosition);

    return () => {
      resizeObserver.disconnect();
      window.removeEventListener("resize", updatePosition);
    };
  }, [anchor, coverage, title]);
  const coverageByFile = groupRowGroupCoverageByFile(coverage);

  return (
    <span
      className={`${styles.columnRowGroupTooltip} calcite-mode-dark`}
      onMouseEnter={keepTooltipHierarchyOpen}
      onMouseLeave={() => {
        scheduleRowGroupTooltipClose();
        onMouseLeave();
      }}
      ref={tooltipRef}
      role="tooltip"
      style={tooltipStyle}
    >
      <span className={styles.columnRowGroupTooltipTitle}>{title}</span>
      <span className={styles.columnRowGroupTooltipFiles}>
        {coverageByFile.map(({ fileName, rowGroups }) => (
          <span className={styles.columnRowGroupTooltipFile} key={fileName}>
            <span
              className={styles.columnRowGroupTooltipFileName}
              title={formatParquetFileName(fileName)}
            >
              {formatParquetFileName(fileName)}
            </span>
            <span className={styles.columnRowGroupTooltipGrid}>
              {rowGroups.map((rowGroup) => (
                <span
                  className={[
                    styles.columnRowGroupTooltipCell,
                    hoveredRowGroup?.rowGroupIndex === rowGroup.rowGroupIndex
                      ? styles.selected
                      : null,
                  ].filter(Boolean).join(" ")}
                  key={rowGroup.rowGroupIndex}
                  onMouseEnter={(event) => {
                    keepTooltipHierarchyOpen();
                    setHoveredRowGroup({
                      element: event.currentTarget,
                      rowGroupIndex: rowGroup.rowGroupIndex,
                    });
                    onRowGroupHover(rowGroup.rowGroupIndex);
                  }}
                  onMouseLeave={scheduleRowGroupTooltipClose}
                  style={{
                    "--row-group-download-percent": `${rowGroup.downloadedPercent}%`,
                  } as CSSProperties}
                >
                  {formatDownloadPercent(rowGroup.downloadedPercent)}
                  {hoveredRowGroup?.rowGroupIndex === rowGroup.rowGroupIndex ? (
                    createPortal(
                      <RowGroupDetailTooltip
                        anchor={hoveredRowGroup.element}
                        coverage={rowGroup}
                        onMouseEnter={keepTooltipHierarchyOpen}
                        onMouseLeave={() => {
                          scheduleRowGroupTooltipClose();
                          onMouseLeave();
                        }}
                      />,
                      document.body,
                    )
                  ) : null}
                </span>
              ))}
            </span>
          </span>
        ))}
      </span>
    </span>
  );
});

const RowGroupDetailTooltip = memo(function RowGroupDetailTooltip({
  anchor,
  coverage,
  onMouseEnter,
  onMouseLeave,
}: {
  anchor: HTMLSpanElement;
  coverage: DownloadRowGroupCoverage;
  onMouseEnter: () => void;
  onMouseLeave: () => void;
}) {
  const tooltipRef = useRef<HTMLSpanElement>(null);
  const [tooltipStyle, setTooltipStyle] = useState<CSSProperties>({
    visibility: "hidden",
  });
  const [hoveredItem, setHoveredItem] = useState<{
    element: HTMLSpanElement;
    label: string;
  } | null>(null);
  const closeItemTooltipTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => () => {
    if (closeItemTooltipTimerRef.current !== null) {
      clearTimeout(closeItemTooltipTimerRef.current);
    }
  }, []);

  const cancelItemTooltipClose = () => {
    if (closeItemTooltipTimerRef.current !== null) {
      clearTimeout(closeItemTooltipTimerRef.current);
      closeItemTooltipTimerRef.current = null;
    }
  };
  const scheduleItemTooltipClose = () => {
    cancelItemTooltipClose();
    closeItemTooltipTimerRef.current = setTimeout(() => {
      closeItemTooltipTimerRef.current = null;
      setHoveredItem(null);
    }, tooltipGracePeriodMs);
  };

  useLayoutEffect(() => {
    const tooltip = tooltipRef.current;
    if (!tooltip) {
      return;
    }

    const updatePosition = () => {
      setTooltipStyle(createAdjacentTooltipStyle(anchor, tooltip, "left"));
    };
    updatePosition();

    const resizeObserver = new ResizeObserver(updatePosition);
    resizeObserver.observe(tooltip);
    window.addEventListener("resize", updatePosition);

    return () => {
      resizeObserver.disconnect();
      window.removeEventListener("resize", updatePosition);
    };
  }, [anchor, coverage]);

  return (
    <span
      className={`${styles.rowGroupDetailTooltip} calcite-mode-dark`}
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
      ref={tooltipRef}
      role="tooltip"
      style={tooltipStyle}
    >
      <span>
        Row group {coverage.sourceRowGroupIndex ?? coverage.rowGroupIndex}
      </span>
      <span>
        {formatByteSize(coverage.downloadedByteLength)}
        {" / "}
        {formatByteSize(coverage.byteLength)}
      </span>
      <span>{formatPreciseDownloadPercent(coverage.downloadedPercent)}</span>
      {coverage.statistics ? (
        <ColumnStatistics statistics={coverage.statistics} />
      ) : null}
      {coverage.itemCoverage.length > 0 ? (
        <>
          <span className={styles.pageIndexColumnTitle}>Columns</span>
          <span className={styles.pageIndexColumnGrid}>
            {coverage.itemCoverage.map((itemCoverage) => {
              const status = itemCoverage.downloaded ? "downloaded" : "not downloaded";
              return (
                <span
                  aria-label={`${itemCoverage.label}: ${status}`}
                  className={[
                    styles.pageIndexColumnCell,
                    itemCoverage.downloaded ? styles.downloaded : null,
                  ].filter(Boolean).join(" ")}
                  key={itemCoverage.label}
                  onMouseEnter={(event) => {
                    cancelItemTooltipClose();
                    onMouseEnter();
                    setHoveredItem({
                      element: event.currentTarget,
                      label: itemCoverage.label,
                    });
                  }}
                  onMouseLeave={scheduleItemTooltipClose}
                >
                  {hoveredItem?.label === itemCoverage.label ? (
                    createPortal(
                      <PageIndexColumnDetailTooltip
                        anchor={hoveredItem.element}
                        coverage={itemCoverage}
                        onMouseEnter={() => {
                          cancelItemTooltipClose();
                          onMouseEnter();
                        }}
                        onMouseLeave={() => {
                          scheduleItemTooltipClose();
                          onMouseLeave();
                        }}
                      />,
                      document.body,
                    )
                  ) : null}
                </span>
              );
            })}
          </span>
        </>
      ) : null}
    </span>
  );
});

function groupRowGroupCoverageByFile(
  coverage: readonly DownloadRowGroupCoverage[],
): Array<{
  fileName: string;
  rowGroups: DownloadRowGroupCoverage[];
}> {
  const groups = new Map<string, DownloadRowGroupCoverage[]>();
  for (const rowGroup of coverage) {
    const fileName = rowGroup.fileName ?? "File";
    const rowGroups = groups.get(fileName) ?? [];
    rowGroups.push(rowGroup);
    groups.set(fileName, rowGroups);
  }
  return Array.from(groups, ([fileName, rowGroups]) => ({
    fileName,
    rowGroups,
  }));
}

const PageIndexColumnDetailTooltip = memo(function PageIndexColumnDetailTooltip({
  anchor,
  coverage,
  onMouseEnter,
  onMouseLeave,
}: {
  anchor: HTMLSpanElement;
  coverage: DownloadRowGroupItemCoverage;
  onMouseEnter: () => void;
  onMouseLeave: () => void;
}) {
  const tooltipRef = useRef<HTMLSpanElement>(null);
  const [tooltipStyle, setTooltipStyle] = useState<CSSProperties>({
    visibility: "hidden",
  });

  useLayoutEffect(() => {
    const tooltip = tooltipRef.current;
    if (!tooltip) {
      return;
    }

    const updatePosition = () => {
      setTooltipStyle(createAdjacentTooltipStyle(anchor, tooltip));
    };
    updatePosition();

    const resizeObserver = new ResizeObserver(updatePosition);
    resizeObserver.observe(tooltip);
    window.addEventListener("resize", updatePosition);

    return () => {
      resizeObserver.disconnect();
      window.removeEventListener("resize", updatePosition);
    };
  }, [anchor, coverage]);

  return (
    <span
      className={`${styles.pageIndexColumnDetailTooltip} calcite-mode-dark`}
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
      ref={tooltipRef}
      role="tooltip"
      style={tooltipStyle}
    >
      <span>{coverage.label}</span>
      <span>
        {formatByteSize(coverage.downloadedByteLength)}
        {" / "}
        {formatByteSize(coverage.byteLength)}
      </span>
      <ColumnStatistics statistics={coverage} />
    </span>
  );
});

const ColumnStatistics = memo(function ColumnStatistics({
  statistics,
}: {
  statistics: DownloadColumnStatistics;
}) {
  return (
    <span className={styles.pageIndexColumnStatistics}>
      <span>Min</span>
      <strong>{formatColumnStatisticValue(statistics.minimumValue)}</strong>
      <span>Max</span>
      <strong>{formatColumnStatisticValue(statistics.maximumValue)}</strong>
      <span>Null count</span>
      <strong>{formatColumnStatisticCount(statistics.nullCount)}</strong>
      <span>Record count</span>
      <strong>{formatColumnStatisticCount(statistics.recordCount)}</strong>
    </span>
  );
});

function useDownloadTrackTooltip({
  onClose,
  onOpen,
  tooltipActive,
}: {
  onClose(): void;
  onOpen(): void;
  tooltipActive: boolean;
}) {
  const [hoveredBlock, setHoveredBlock] = useState<HTMLSpanElement | null>(null);
  const [hoveredRowGroupIndex, setHoveredRowGroupIndex] =
    useState<number | null>(null);
  const openTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const closeTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => () => {
    if (openTimerRef.current !== null) {
      clearTimeout(openTimerRef.current);
    }
    if (closeTimerRef.current !== null) {
      clearTimeout(closeTimerRef.current);
    }
  }, []);

  const cancelClose = () => {
    if (closeTimerRef.current !== null) {
      clearTimeout(closeTimerRef.current);
      closeTimerRef.current = null;
    }
  };
  const cancelOpen = () => {
    if (openTimerRef.current !== null) {
      clearTimeout(openTimerRef.current);
      openTimerRef.current = null;
    }
  };
  const scheduleOpen = (element: HTMLSpanElement) => {
    cancelClose();
    cancelOpen();
    if (hoveredBlock && tooltipActive) {
      return;
    }
    openTimerRef.current = setTimeout(() => {
      openTimerRef.current = null;
      onOpen();
      setHoveredRowGroupIndex(null);
      setHoveredBlock(
        element.parentElement?.querySelector<HTMLSpanElement>(
          `.${styles.chunk}`,
        ) ?? element,
      );
    }, tooltipOpenDelayMs);
  };
  const scheduleClose = () => {
    cancelOpen();
    cancelClose();
    closeTimerRef.current = setTimeout(() => {
      closeTimerRef.current = null;
      setHoveredBlock(null);
      setHoveredRowGroupIndex(null);
      onClose();
    }, tooltipGracePeriodMs);
  };

  return {
    cancelClose,
    cancelOpen,
    hoveredBlock,
    hoveredRowGroupIndex,
    scheduleClose,
    scheduleOpen,
    setHoveredRowGroupIndex,
  };
}

function formatColumnStatisticValue(
  value: number | string | null | undefined,
): string {
  if (
    value === null ||
    value === undefined ||
    (typeof value === "number" && !Number.isFinite(value))
  ) {
    return "Unavailable";
  }
  return typeof value === "number"
    ? new Intl.NumberFormat(undefined, { maximumSignificantDigits: 8 }).format(value)
    : value;
}

function formatColumnStatisticCount(value: number | null | undefined): string {
  return value === null || value === undefined || !Number.isFinite(value)
    ? "Unavailable"
    : formatInteger(value);
}

function createColumnTooltipStyle(
  anchor: HTMLSpanElement,
  tooltip: HTMLSpanElement,
): CSSProperties {
  const anchorBounds = anchor.getBoundingClientRect();
  const tooltipBounds = tooltip.getBoundingClientRect();
  const viewportPadding = 12;
  const horizontalGap = 8;
  const horizontalOffset = 48;
  const fitsLeft =
    anchorBounds.left - horizontalGap - tooltipBounds.width >= viewportPadding;
  const left = fitsLeft
    ? anchorBounds.left - horizontalGap - tooltipBounds.width
    : Math.min(
        anchorBounds.right + horizontalGap,
        window.innerWidth - tooltipBounds.width - viewportPadding,
      );
  const top = Math.min(
    Math.max(
      anchorBounds.top + anchorBounds.height / 2 - tooltipBounds.height / 2,
      viewportPadding,
    ),
    window.innerHeight - tooltipBounds.height - viewportPadding,
  );

  return {
    left: Math.max(left - horizontalOffset, viewportPadding),
    top: Math.max(top, viewportPadding),
    visibility: "visible",
  };
}

function formatDownloadPercent(percent: number): string {
  return formatPercent(percent);
}

function formatPreciseDownloadPercent(percent: number): string {
  return formatPercent(percent, 2);
}

function createAdjacentTooltipStyle(
  anchor: HTMLSpanElement,
  tooltip: HTMLSpanElement,
  preferredSide: "left" | "right" = "right",
): CSSProperties {
  const anchorBounds = anchor.getBoundingClientRect();
  const tooltipBounds = tooltip.getBoundingClientRect();
  const viewportPadding = 8;
  const horizontalGap = 8;
  const leftPosition =
    anchorBounds.left - horizontalGap - tooltipBounds.width;
  const rightPosition = anchorBounds.right + horizontalGap;
  const fitsLeft = leftPosition >= viewportPadding;
  const fitsRight =
    rightPosition + tooltipBounds.width <=
    window.innerWidth - viewportPadding;
  const left =
    preferredSide === "left"
      ? fitsLeft || !fitsRight
        ? leftPosition
        : rightPosition
      : fitsRight || !fitsLeft
        ? rightPosition
        : leftPosition;
  const top = Math.min(
    Math.max(anchorBounds.top, viewportPadding),
    window.innerHeight - tooltipBounds.height - viewportPadding,
  );
  return {
    left: Math.max(left, viewportPadding),
    top: Math.max(top, viewportPadding),
    visibility: "visible",
  };
}

function byteLengthToThresholdSliderValue(
  byteLength: number,
  maximumByteLength: number,
): number {
  if (byteLength <= 0) {
    return 0;
  }
  const logarithmicRange = Math.log(maximumByteLength / minimumPositiveColumnThreshold);
  if (logarithmicRange <= 0) {
    return columnThresholdSliderMaximum;
  }
  return Math.round(
    (Math.log(byteLength / minimumPositiveColumnThreshold) / logarithmicRange) *
      (columnThresholdSliderMaximum - 1) +
      1,
  );
}

function thresholdSliderValueToByteLength(
  sliderValue: number,
  maximumByteLength: number,
): number {
  if (sliderValue <= 0) {
    return 0;
  }
  const logarithmicProgress =
    (sliderValue - 1) / (columnThresholdSliderMaximum - 1);
  return Math.round(
    minimumPositiveColumnThreshold *
      Math.exp(
        Math.log(maximumByteLength / minimumPositiveColumnThreshold) *
          logarithmicProgress,
      ),
  );
}

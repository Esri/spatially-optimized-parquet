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

import {
  resolveParquetPageIndexSource,
  type ArcgisParquetPageIndexSource,
} from "./inspector/arcgisPageIndexes";
import { formatByteSize } from "../../common/formatByteSize";
import type {
  DownloadBlockLayout,
  DownloadTrackLayout,
} from "../../parquet/displayLayout";
import type { RowGroupBounds } from "../../parquet/rowGroupBounds";
import { InspectorDialog } from "./inspector/InspectorDialog";
import type { FileStructureSnapshot } from "./inspector/ParquetFileStructureStore";
import { Minimap } from "./minimap/Minimap";
import {
  type DownloadColumnStatistics,
  type DownloadRowGroupItemCoverage,
  type DownloadRowGroupCoverage,
  type DownloadSessionView,
} from "./download/ParquetDownloadSession";

const columnThresholdSliderMaximum = 1000;
const minimumPositiveColumnThreshold = 1024;
const defaultColumnDownloadThreshold = 250 * 1024;
const tooltipOpenDelayMs = 100;
const tooltipGracePeriodMs = 200;

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
  return <span className={`chunk-subpart ${state}${highlighted ? " selected" : ""}`} />;
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
      className="chunk"
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

const DownloadTrackProgress = memo(function DownloadTrackProgress({
  track,
  downloadedByteLength,
}: {
  track: DownloadTrackLayout;
  downloadedByteLength: number;
}) {
  return (
    <span className="column-progress">
      ({formatByteSize(downloadedByteLength)}/{formatByteSize(track.byteLength)})
    </span>
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
  const [hoveredBlock, setHoveredBlock] = useState<HTMLSpanElement | null>(null);
  const [hoveredRowGroupIndex, setHoveredRowGroupIndex] = useState<number | null>(null);
  const openTooltipTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const closeTooltipTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => () => {
    if (openTooltipTimerRef.current !== null) {
      clearTimeout(openTooltipTimerRef.current);
    }
    if (closeTooltipTimerRef.current !== null) {
      clearTimeout(closeTooltipTimerRef.current);
    }
  }, []);
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

  const rowGroupCoverage = hoveredBlock && tooltipActive
    ? downloadSession.rowGroupCoverage(track.id)
    : [];
  const highlightedBlockMasks = hoveredRowGroupIndex === null || !tooltipActive
    ? undefined
    : downloadSession.rowGroupBlockMasks(track.id, hoveredRowGroupIndex);
  const cancelTooltipClose = () => {
    if (closeTooltipTimerRef.current !== null) {
      clearTimeout(closeTooltipTimerRef.current);
      closeTooltipTimerRef.current = null;
    }
  };
  const cancelTooltipOpen = () => {
    if (openTooltipTimerRef.current !== null) {
      clearTimeout(openTooltipTimerRef.current);
      openTooltipTimerRef.current = null;
    }
  };
  const scheduleTooltipOpen = (element: HTMLSpanElement) => {
    cancelTooltipClose();
    cancelTooltipOpen();
    if (hoveredBlock && tooltipActive) {
      return;
    }
    openTooltipTimerRef.current = setTimeout(() => {
      openTooltipTimerRef.current = null;
      onTooltipOpen();
      setHoveredRowGroupIndex(null);
      const firstAggregateBlock =
        element.parentElement?.querySelector<HTMLSpanElement>(".chunk") ?? element;
      setHoveredBlock(firstAggregateBlock);
    }, tooltipOpenDelayMs);
  };
  const scheduleTooltipClose = () => {
    cancelTooltipOpen();
    cancelTooltipClose();
    closeTooltipTimerRef.current = setTimeout(() => {
      closeTooltipTimerRef.current = null;
      setHoveredBlock(null);
      setHoveredRowGroupIndex(null);
      onTooltipClose();
    }, tooltipGracePeriodMs);
  };

  return (
    <span
      className={`column-range${
        hoveredBlock && tooltipActive ? " column-range-selected" : ""
      }`}
      onMouseLeave={scheduleTooltipClose}
    >
      <span className="column-range-content">
        <span className="column-label">
          <span className="column-download-share">
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
        <span className="column-blocks">
          {snapshot.visibleBlockIds.map((blockId) => {
            const block = blockById.get(blockId);
            return block ? (
              <DownloadChunk
                block={block}
                downloadSession={downloadSession}
                highlightedMask={highlightedBlockMasks?.get(block.id) ?? 0}
                key={block.id}
                onHover={supportsRowGroupTooltip ? scheduleTooltipOpen : undefined}
                onHoverEnd={supportsRowGroupTooltip ? cancelTooltipOpen : undefined}
              />
            ) : null;
          })}
        </span>
      </span>
      {hoveredBlock && tooltipActive && supportsRowGroupTooltip ? (
        createPortal(
          <TrackRowGroupTooltip
            anchor={hoveredBlock}
            coverage={rowGroupCoverage}
            onMouseEnter={cancelTooltipClose}
            onMouseLeave={scheduleTooltipClose}
            onRowGroupHover={setHoveredRowGroupIndex}
            title={`${track.label} by row group`}
          />,
          document.body,
        )
      ) : null}
    </span>
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

  return (
    <span
      className="column-row-group-tooltip calcite-mode-dark"
      onMouseEnter={keepTooltipHierarchyOpen}
      onMouseLeave={() => {
        scheduleRowGroupTooltipClose();
        onMouseLeave();
      }}
      ref={tooltipRef}
      role="tooltip"
      style={tooltipStyle}
    >
      <span className="column-row-group-tooltip-title">{title}</span>
      <span className="column-row-group-tooltip-grid">
        {coverage.map((rowGroup) => (
          <span
            className={`column-row-group-tooltip-cell${
              hoveredRowGroup?.rowGroupIndex === rowGroup.rowGroupIndex
                ? " selected"
                : ""
            }`}
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
      className="row-group-detail-tooltip calcite-mode-dark"
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
      ref={tooltipRef}
      role="tooltip"
      style={tooltipStyle}
    >
      <span>Row group {coverage.rowGroupIndex}</span>
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
          <span className="page-index-column-title">Columns</span>
          <span className="page-index-column-grid">
            {coverage.itemCoverage.map((itemCoverage) => {
              const status = itemCoverage.downloaded ? "downloaded" : "not downloaded";
              return (
                <span
                  aria-label={`${itemCoverage.label}: ${status}`}
                  className={`page-index-column-cell${
                    itemCoverage.downloaded ? " downloaded" : ""
                  }`}
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
      className="page-index-column-detail-tooltip calcite-mode-dark"
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
    <span className="page-index-column-statistics">
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
    : new Intl.NumberFormat().format(value);
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
  return `${Math.round(percent)}%`;
}

function formatPreciseDownloadPercent(percent: number): string {
  return `${new Intl.NumberFormat(undefined, { maximumFractionDigits: 2 }).format(percent)}%`;
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
    <div className="bytes-loaded">
      <div className="file-stat-label">Bytes Loaded</div>
      <div className="file-stat-value">
        {formatByteSize(summary.downloadedByteLength)}
        <span className="file-stat-unit">
          / {byteLength === null ? "…" : formatByteSize(byteLength)}
        </span>
      </div>
    </div>
  );
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
    <div className="occupancy-footer">
      <span className="occupancy-summary">
        {visibleColumnCount}/{summary.columnCount} columns
      </span>
      <span className="occupancy-legend" aria-label="Download state">
        <span>
          <i className="loaded" aria-hidden="true" />
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
    <label className="column-threshold-control">
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
        <div className="column-threshold-frame">
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
      <div className="occupancy-grid-frame">
        <div className="occupancy-flow" role="img" aria-label="Parquet download coverage by track">
          {topology.layout ? (
            <DownloadTrackList
              downloadSession={downloadSession}
              minimumDownloadedByteLength={minimumDownloadedByteLength}
              tracks={topology.tracks}
            />
          ) : (
            <span className="occupancy-status">
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

export interface FileExplorerProps {
  basemap?: string;
  center: [number, number];
  datasetId: string;
  downloadSession: DownloadSessionView;
  layout: "desktop" | "compact";
  mainMapElementRef: RefObject<HTMLArcgisMapElement | null>;
  parquetSource: unknown | null;
  rowGroupBounds: readonly RowGroupBounds[] | null;
  scale: number;
  spatialReferenceWkid?: number;
  visible?: boolean;
}

export const FileExplorer = memo(function FileExplorer({
  basemap,
  center,
  datasetId,
  downloadSession,
  layout,
  mainMapElementRef,
  parquetSource,
  rowGroupBounds,
  scale,
  spatialReferenceWkid,
  visible = true,
}: FileExplorerProps) {
  const [fileStructureDialog, setFileStructureDialog] = useState<{
    datasetId: string;
    snapshot: FileStructureSnapshot;
    source: ArcgisParquetPageIndexSource;
  } | null>(null);
  const openFileStructure = () => {
    const snapshot = downloadSession.createFileStructureSnapshot();
    if (!snapshot || !parquetSource) {
      return;
    }
    setFileStructureDialog({
      datasetId,
      snapshot,
      source: resolveParquetPageIndexSource(parquetSource),
    });
  };
  const content = (
    <>
      <div className="file-explorer-overview">
        <Minimap
          basemapId={basemap}
          bounds={rowGroupBounds}
          center={center}
          key={datasetId}
          mainMapElementRef={mainMapElementRef}
          scale={scale}
          spatialReferenceWkid={spatialReferenceWkid}
        />
        <div className="file-structure-action">
          <calcite-button
            appearance="solid"
            className="file-structure-button"
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
      {layout === "desktop" ? (
        <div className="grid-details-column">{content}</div>
      ) : visible ? (
        <aside aria-label="Details" className="responsive-details-overlay">
          {content}
        </aside>
      ) : null}
      {fileStructureDialog?.datasetId === datasetId ? (
        createPortal(
          <InspectorDialog
            onClose={() => setFileStructureDialog(null)}
            snapshot={fileStructureDialog.snapshot}
            source={fileStructureDialog.source}
          />,
          document.body,
        )
      ) : null}
    </>
  );
});

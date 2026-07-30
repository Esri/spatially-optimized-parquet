import ParquetLayer from "@arcgis/core/layers/ParquetLayer";
import FeatureLayer from "@arcgis/core/layers/FeatureLayer";
import GraphicsLayer from "@arcgis/core/layers/GraphicsLayer";
import VectorTileLayer from "@arcgis/core/layers/VectorTileLayer";
import ParquetFilesData from "@arcgis/core/layers/support/ParquetFilesData";
import Graphic from "@arcgis/core/Graphic";
import Extent from "@arcgis/core/geometry/Extent";
import SpatialReference from "@arcgis/core/geometry/SpatialReference";
import MapViewConstraints from "@arcgis/core/views/2d/MapViewConstraints";
import Viewpoint from "@arcgis/core/Viewpoint";
import Basemap from "@arcgis/core/Basemap";
import * as reactiveUtils from "@arcgis/core/core/reactiveUtils";
import {
  memo,
  type CSSProperties,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import { createPortal } from "react-dom";

import { datasets } from "./datasets";
import type { Dataset } from "./datasets";
import { DatasetSelectionPanel } from "./DatasetSelectionPanel";
import { formatByteSize } from "./formatByteSize";
import { deriveFileDetailSummary } from "./parquetFileDetails";
import {
  resolveDatasetMapProfile,
  type DatasetEffectLayer,
  type DatasetMapProfile,
} from "./datasetMapProfiles";
import {
  parseParquetDiagnosticsSnapshot,
  parseParquetRangeReadEvent,
  resolveParquetDiagnosticsSource,
  type ArcgisEventHandle,
  type ArcgisParquetRangeReadEvent,
} from "./arcgisParquetDiagnostics";
import {
  resolveParquetPageIndexSource,
  type ArcgisParquetPageIndexSource,
} from "./arcgisParquetPageIndexes";
import { ParquetFileStructureDialog } from "./ParquetFileStructureDialog";
import type { FileStructureSnapshot } from "./parquetFileStructureStore";
import {
  ParquetDownloadStore,
  type DownloadColumnStatistics,
  type DownloadRowGroupItemCoverage,
  type DownloadRowGroupCoverage,
} from "./parquetDownloadStore";
import type { DownloadBlockLayout, DownloadTrackLayout } from "./parquetDisplayLayout";
import {
  calculateRowGroupFocusExtent,
  type RowGroupBounds,
} from "./parquetRowGroupBounds";

const defaultCenter: [number, number] = [-98, 39];
const defaultScale = 25_000_000;
const downloadDetailsEnabled = true;
const columnThresholdSliderMaximum = 1000;
const minimumPositiveColumnThreshold = 1024;
const defaultColumnDownloadThreshold = 250 * 1024;
const tooltipOpenDelayMs = 100;
const tooltipGracePeriodMs = 200;
const detailsLayoutBreakpoint = 1024;
const censusDatasetName = "Census Blocks, Demographics";

function createDatasetBasemap(basemapId?: string): Basemap | string {
  return basemapId
    ? new Basemap({
        baseLayers: [
          new VectorTileLayer({
            portalItem: { id: basemapId },
          }),
        ],
      })
    : "dark-gray-vector";
}

function formatCompressionSummary(
  summary: ReturnType<typeof deriveFileDetailSummary>,
): string {
  const codec = summary.compressionCodecs.length === 1
    ? summary.compressionCodecs[0].toUpperCase()
    : "Mixed";
  const ratio = summary.compressedSize > 0
    ? summary.uncompressedSize / summary.compressedSize
    : null;
  return ratio ? `${codec} ${ratio.toFixed(1)}x` : codec;
}

function createDatasetSpatialReference(wkid?: number): SpatialReference {
  return new SpatialReference({ wkid: wkid ?? 3857 });
}

function createDebugBoundsSymbol(outlineColor: string) {
  return {
    type: "simple-fill" as const,
    color: [0, 0, 0, 0],
    outline: { color: outlineColor, width: 1 },
  };
}

function hasDatasetEffectLayer(layer: object): layer is DatasetEffectLayer {
  return "effect" in layer;
}

function createDebugLabelClass(colorClass: number, textColor: string) {
  return {
    where: `COLOR_CLASS = ${colorClass}`,
    labelExpressionInfo: { expression: "'RG ' + Text($feature.ROW_GROUP)" },
    labelPlacement: "always-horizontal" as const,
    symbol: {
      type: "text" as const,
      color: textColor,
      haloColor: [0, 0, 0, 0.8],
      haloSize: 2,
      font: { family: "Arial", size: 14, weight: "bold" as const },
    },
  };
}

function createRowGroupBoundsLayer(
  bounds: readonly RowGroupBounds[],
  labelsVisible = true,
): FeatureLayer {
  return new FeatureLayer({
    title: "Parquet row group bounds",
    geometryType: "polygon",
    spatialReference: { wkid: 4326 },
    objectIdField: "OBJECTID",
    legendEnabled: false,
    fields: [
      { name: "OBJECTID", type: "oid" },
      { name: "ROW_GROUP", type: "integer" },
      { name: "COLOR_CLASS", type: "integer" },
    ],
    source: bounds.map(
      ({ rowGroupIndex, xmin, ymin, xmax, ymax }) =>
        new Graphic({
          attributes: {
            OBJECTID: rowGroupIndex + 1,
            ROW_GROUP: rowGroupIndex,
            COLOR_CLASS: rowGroupIndex % 3,
          },
          geometry: {
            type: "polygon",
            rings: [
              [
                [xmin, ymin],
                [xmax, ymin],
                [xmax, ymax],
                [xmin, ymax],
                [xmin, ymin],
              ],
            ],
            spatialReference: { wkid: 4326 },
          },
        }),
    ),
    labelingInfo: labelsVisible
      ? [
          createDebugLabelClass(0, "#ef3573"),
          createDebugLabelClass(1, "#5bff94"),
          createDebugLabelClass(2, "#3ec5ff"),
        ]
      : [],
    renderer: {
      type: "class-breaks",
      field: "COLOR_CLASS",
      classBreakInfos: [
        {
          minValue: -0.5,
          maxValue: 0.5,
          symbol: createDebugBoundsSymbol("#ef3573"),
        },
        {
          minValue: 0.5,
          maxValue: 1.5,
          symbol: createDebugBoundsSymbol("#5bff94"),
        },
        {
          minValue: 1.5,
          maxValue: 2.5,
          symbol: createDebugBoundsSymbol("#3ec5ff"),
        },
      ],
    },
  });
}

const MapCanvas = memo(function MapCanvas({
  dataset,
  headerActionsElement,
  mapElementRef,
  profile,
  layer,
}: {
  dataset: Dataset;
  headerActionsElement: HTMLElement | null;
  mapElementRef: React.RefObject<HTMLArcgisMapElement | null>;
  profile: DatasetMapProfile;
  layer: ParquetLayer | null;
}) {
  const MapSlotComponent = profile.mapSlotComponent;
  const basemap = useMemo(
    () => createDatasetBasemap(dataset.basemap),
    [dataset.basemap],
  );
  const spatialReference = useMemo(
    () => createDatasetSpatialReference(dataset.spatialReference),
    [dataset.spatialReference],
  );
  const constraints = useMemo(
    () => new MapViewConstraints({ minScale: dataset.scale * 4 }),
    [dataset.scale],
  );

  return (
    <arcgis-map
      ref={mapElementRef}
      aria-label={`${dataset.name} map`}
      spatialReference={spatialReference}
      basemap={basemap}
      constraints={constraints}
      center={defaultCenter}
      scale={defaultScale}
    >
      {MapSlotComponent ? (
        <MapSlotComponent
          headerActionsElement={headerActionsElement}
          key={layer?.id ?? "empty"}
          layer={layer}
        />
      ) : null}
    </arcgis-map>
  );
});

const RowGroupOverviewMap = memo(function RowGroupOverviewMap({
  basemapId,
  bounds,
  center,
  mainMapElementRef,
  scale,
  spatialReferenceWkid,
}: {
  basemapId?: string;
  bounds: readonly RowGroupBounds[] | null;
  center: [number, number];
  mainMapElementRef: React.RefObject<HTMLArcgisMapElement | null>;
  scale: number;
  spatialReferenceWkid?: number;
}) {
  const mapElementRef = useRef<HTMLArcgisMapElement>(null);
  const [overviewReady, setOverviewReady] = useState(false);
  const basemap = useMemo(
    () => createDatasetBasemap(basemapId),
    [basemapId],
  );
  const spatialReference = useMemo(
    () => createDatasetSpatialReference(spatialReferenceWkid),
    [spatialReferenceWkid],
  );

  useEffect(() => {
    const mapElement = mapElementRef.current;
    if (!mapElement || !bounds) {
      return;
    }

    mapElement.center = center;
    mapElement.scale = scale;

    let disposed = false;
    let boundsLayer: FeatureLayer | undefined;
    let extentLayer: GraphicsLayer | undefined;
    let extentWatcher: { remove(): void } | undefined;
    let overviewClickHandle: { remove(): void } | undefined;
    let overviewDragHandle: { remove(): void } | undefined;

    const loadOverview = async () => {
      try {
        await mapElement.viewOnReady();
        const map = mapElement.map;
        if (!map) {
          throw new Error("Row group overview map is unavailable.");
        }
        boundsLayer = createRowGroupBoundsLayer(bounds, false);
        map.add(boundsLayer);
        await boundsLayer.when();
        const focusExtent = calculateRowGroupFocusExtent(bounds);
        if (!disposed && focusExtent) {
          await mapElement.view.goTo(
            new Extent({
              ...focusExtent,
              spatialReference: { wkid: 4326 },
            }),
            { animate: false },
          );
        }
        if (!disposed) {
          setOverviewReady(true);
        }

        const mainMapElement = mainMapElementRef.current;
        if (!mainMapElement || disposed) {
          return;
        }
        await mainMapElement.viewOnReady();
        if (disposed) {
          return;
        }

        const recenterMainView = (x: number, y: number) => {
          const center = mapElement.view.toMap({ x, y });
          if (center) {
            mainMapElement.view.center = center;
          }
        };
        overviewClickHandle = mapElement.view.on(
          "immediate-click",
          (event) => {
            if (event.button === 0) {
              recenterMainView(event.x, event.y);
            }
          },
        );
        overviewDragHandle = mapElement.view.on("drag", (event) => {
          if (event.button === 0) {
            event.stopPropagation();
            recenterMainView(event.x, event.y);
          }
        });

        const extentGraphic = new Graphic({
          geometry: mainMapElement.view.extent.clone(),
          symbol: {
            type: "simple-fill",
            color: [255, 255, 255, 0.2],
            outline: { color: [255, 255, 255, 1], width: 1.5 },
          },
        });
        extentLayer = new GraphicsLayer({
          title: "Current view extent",
          graphics: [extentGraphic],
        });
        map.add(extentLayer);
        extentWatcher = reactiveUtils.watch(
          () => mainMapElement.view.extent,
          (extent) => {
            extentGraphic.geometry = extent.clone();
          },
        );
      } catch (error) {
        if (!disposed) {
          console.error("Failed to load row group overview map.", error);
        }
      }
    };

    void loadOverview();

    return () => {
      disposed = true;
      overviewClickHandle?.remove();
      overviewDragHandle?.remove();
      extentWatcher?.remove();
      if (extentLayer) {
        mapElement.map?.remove(extentLayer);
      }
      if (boundsLayer) {
        mapElement.map?.remove(boundsLayer);
      }
    };
  }, [bounds, center, mainMapElementRef, scale]);

  return (
    <div className="row-group-overview">
      <div className="row-group-overview-map-frame">
        {bounds ? (
          <arcgis-map
            ref={mapElementRef}
            aria-label="Parquet row group overview"
            className={overviewReady ? "ready" : ""}
            spatialReference={spatialReference}
            basemap={basemap}
            center={center}
            scale={scale}
          />
        ) : null}
        {!bounds || !overviewReady ? (
          <div className="row-group-overview-placeholder">
            Loading row groups…
          </div>
        ) : null}
      </div>
    </div>
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
  return <span className={`chunk-subpart ${state}${highlighted ? " selected" : ""}`} />;
});

const DownloadChunk = memo(function DownloadChunk({
  block,
  downloadStore,
  highlightedMask = 0,
  onHover,
  onHoverEnd,
}: {
  block: DownloadBlockLayout;
  downloadStore: ParquetDownloadStore;
  highlightedMask?: number;
  onHover?: (element: HTMLSpanElement) => void;
  onHoverEnd?: () => void;
}) {
  const subscribe = useCallback(
    (listener: () => void) => downloadStore.subscribeBlock(block.id, listener),
    [block.id, downloadStore],
  );
  const getSnapshot = useCallback(
    () => downloadStore.getBlockSnapshot(block.id),
    [block.id, downloadStore],
  );
  const snapshot = useSyncExternalStore(subscribe, getSnapshot);
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
  downloadStore,
  minimumDownloadedByteLength,
  onTooltipClose,
  onTooltipOpen,
  tooltipActive,
  totalDownloadedByteLength,
}: {
  track: DownloadTrackLayout;
  downloadStore: ParquetDownloadStore;
  minimumDownloadedByteLength: number;
  onTooltipClose: () => void;
  onTooltipOpen: () => void;
  tooltipActive: boolean;
  totalDownloadedByteLength: number;
}) {
  const subscribe = useCallback(
    (listener: () => void) => downloadStore.subscribeTrack(track.id, listener),
    [downloadStore, track.id],
  );
  const getSnapshot = useCallback(
    () => downloadStore.getTrackSnapshot(track.id),
    [downloadStore, track.id],
  );
  const snapshot = useSyncExternalStore(subscribe, getSnapshot);
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
    ? downloadStore.getTrackRowGroupCoverage(track.id)
    : [];
  const highlightedBlockMasks = hoveredRowGroupIndex === null || !tooltipActive
    ? undefined
    : downloadStore.getTrackRowGroupBlockMasks(track.id, hoveredRowGroupIndex);
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
                downloadStore={downloadStore}
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
  downloadStore,
  byteLength,
}: {
  downloadStore: ParquetDownloadStore;
  byteLength: number | null;
}) {
  const summary = useSyncExternalStore(
    downloadStore.subscribeSummary,
    downloadStore.getSummarySnapshot,
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
  downloadStore,
  minimumDownloadedByteLength,
  tracks,
}: {
  downloadStore: ParquetDownloadStore;
  minimumDownloadedByteLength: number;
  tracks: readonly DownloadTrackLayout[];
}) {
  const summary = useSyncExternalStore(
    downloadStore.subscribeSummary,
    downloadStore.getSummarySnapshot,
  );
  const visibleColumnCount = tracks.filter((track) => {
    if (track.kind !== "column") {
      return false;
    }
    const snapshot = downloadStore.getTrackSnapshot(track.id);
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
  downloadStore,
  minimumDownloadedByteLength,
  tracks,
}: {
  downloadStore: ParquetDownloadStore;
  minimumDownloadedByteLength: number;
  tracks: readonly DownloadTrackLayout[];
}) {
  const [activeTooltipTrackId, setActiveTooltipTrackId] = useState<string | null>(null);
  const summary = useSyncExternalStore(
    downloadStore.subscribeSummary,
    downloadStore.getSummarySnapshot,
  );
  const trackOrder = new Map(tracks.map((track, index) => [track.id, index]));
  const sortedTracks = [...tracks].sort((first, second) => {
    const downloadedByteDifference =
      downloadStore.getTrackSnapshot(second.id).downloadedByteLength -
      downloadStore.getTrackSnapshot(first.id).downloadedByteLength;
    return downloadedByteDifference !== 0
      ? downloadedByteDifference
      : (trackOrder.get(first.id) ?? 0) - (trackOrder.get(second.id) ?? 0);
  });

  return sortedTracks.map((track) => (
    <DownloadTrack
      downloadStore={downloadStore}
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
  downloadStore,
}: {
  downloadStore: ParquetDownloadStore;
}) {
  const topology = useSyncExternalStore(
    downloadStore.subscribeTopology,
    downloadStore.getTopologySnapshot,
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
            downloadStore={downloadStore}
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
              downloadStore={downloadStore}
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
          downloadStore={downloadStore}
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

function formatFeatureCount(featureCount: number | null): string {
  if (featureCount === null) {
    return "…";
  }

  return new Intl.NumberFormat(undefined, {
    maximumFractionDigits: 1,
    notation: "compact",
  }).format(featureCount);
}

export function ArcgisViewer() {
  const [datasetIndex, setDatasetIndex] = useState(0);
  const [mapReady, setMapReady] = useState(false);
  const [viewCenter, setViewCenter] = useState<{
    latitude: number;
    longitude: number;
  } | null>(null);
  const [layerViewFeatureCount, setLayerViewFeatureCount] = useState<number | null>(null);
  const [parquetLayer, setParquetLayer] = useState<ParquetLayer | null>(null);
  const [rowGroupBounds, setRowGroupBounds] =
    useState<readonly RowGroupBounds[] | null>(null);
  const [debugEnabled, setDebugEnabled] = useState(false);
  const [compactDetailsLayout, setCompactDetailsLayout] = useState(false);
  const [responsiveDetailsOpen, setResponsiveDetailsOpen] = useState(false);
  const [mapHeaderActionsElement, setMapHeaderActionsElement] =
    useState<HTMLDivElement | null>(null);
  const [bookmarksButton, setBookmarksButton] =
    useState<HTMLCalciteButtonElement | null>(null);
  const [bookmarksOpen, setBookmarksOpen] = useState(false);
  const [fileStructureDialog, setFileStructureDialog] = useState<{
    snapshot: FileStructureSnapshot;
    source: ArcgisParquetPageIndexSource;
  } | null>(null);
  const mapElementRef = useRef<HTMLArcgisMapElement>(null);
  const gridContainerRef = useRef<HTMLElement>(null);
  const parquetLayerRef = useRef<ParquetLayer | null>(null);
  const parquetSourceRef = useRef<unknown>(null);
  const downloadStoreRef = useRef<ParquetDownloadStore | null>(null);
  if (!downloadStoreRef.current) {
    downloadStoreRef.current = new ParquetDownloadStore();
  }

  const downloadStore = downloadStoreRef.current;
  const activeDataset = datasets[datasetIndex];
  const activeProfile = resolveDatasetMapProfile(activeDataset.id);
  const datasetByteSize =
    downloadStore.getSnapshot().layout?.byteLength ?? activeDataset.byteSize;
  const fileLayout = downloadStore.getSnapshot().layout;
  const compression = fileLayout
    ? formatCompressionSummary(deriveFileDetailSummary(fileLayout))
    : null;
  const selectDataset = (index: number) => {
    setResponsiveDetailsOpen(false);
    if (index === datasetIndex) {
      return;
    }
    setRowGroupBounds(null);
    setDatasetIndex(index);
  };
  const openFileStructure = () => {
    const snapshot = downloadStore.createFileStructureSnapshot();
    if (!snapshot || !parquetSourceRef.current) {
      return;
    }
    setFileStructureDialog({
      snapshot,
      source: resolveParquetPageIndexSource(parquetSourceRef.current),
    });
  };

  useLayoutEffect(() => {
    if (!downloadDetailsEnabled) {
      return;
    }

    const gridContainer = gridContainerRef.current;
    if (!gridContainer) {
      return;
    }

    const updateLayout = (width: number) => {
      const compact = width <= detailsLayoutBreakpoint;
      setCompactDetailsLayout(compact);
      if (!compact) {
        setResponsiveDetailsOpen(false);
      }
    };
    updateLayout(gridContainer.getBoundingClientRect().width);

    const resizeObserver = new ResizeObserver(([entry]) => {
      updateLayout(entry.contentRect.width);
    });
    resizeObserver.observe(gridContainer);

    return () => {
      resizeObserver.disconnect();
    };
  }, []);

  useEffect(() => {
    const mapElement = mapElementRef.current;
    if (!mapElement) {
      return;
    }

    const markMapReady = () => setMapReady(true);
    mapElement.addEventListener("arcgisViewReadyChange", markMapReady);

    if (mapElement.ready) {
      markMapReady();
    }

    return () => {
      mapElement.removeEventListener("arcgisViewReadyChange", markMapReady);
    };
  }, []);

  useEffect(() => {
    const mapElement = mapElementRef.current;
    const map = mapElement?.map;
    if (!mapReady || !mapElement || !map || !activeDataset.url) {
      return;
    }

    setLayerViewFeatureCount(null);
    setRowGroupBounds(null);
    downloadStore.reset();
    mapElement.center = activeDataset.center ?? defaultCenter;
    mapElement.scale = activeDataset.scale ?? defaultScale;
    map.layers.removeAll();
    const layer = new ParquetLayer({
      title: activeDataset.name,
      copyright: activeDataset.source,
      data: new ParquetFilesData({ urls: [activeDataset.url] }),
      maxScale: activeDataset.maxScale,
      ...activeProfile.layerProperties,
    });
    parquetLayerRef.current = layer;
    setParquetLayer(layer);
    map.add(layer);

    let disposed = false;
    let layerViewWatcher: { remove(): void } | undefined;
    let rangeReadHandle: ArcgisEventHandle | undefined;
    const profileLayerCleanup = hasDatasetEffectLayer(layer)
      ? activeProfile.configureLayer?.(layer)
      : undefined;
    const reportDiagnosticsError = (message: string, error: unknown) => {
      if (disposed) {
        return;
      }
      const diagnosticsError =
        error instanceof Error ? error : new Error(message);
      downloadStore.setError(diagnosticsError);
      console.error(message, error);
    };
    const handleParsedRangeRead = (event: ArcgisParquetRangeReadEvent) => {
      if (!disposed) {
        downloadStore.handleRangeRead(event);
      }
    };
    const initializeLayer = async () => {
      try {
        await layer.when();
        if (disposed) {
          return;
        }

        try {
          const diagnosticsSource = resolveParquetDiagnosticsSource(layer);
          parquetSourceRef.current = diagnosticsSource;
          rangeReadHandle = diagnosticsSource.on("range-read", (event) => {
            try {
              handleParsedRangeRead(parseParquetRangeReadEvent(event));
            } catch (error) {
              reportDiagnosticsError("Failed to parse a Parquet range event.", error);
            }
          });

          const diagnosticsSnapshotRequest =
            diagnosticsSource.getDiagnosticsSnapshot();
          void (async () => {
            try {
              const snapshotValue = await diagnosticsSnapshotRequest;
              if (disposed) {
                return;
              }
              const snapshot = parseParquetDiagnosticsSnapshot(snapshotValue);
              downloadStore.setDiagnostics(snapshot);
              setRowGroupBounds(downloadStore.getSnapshot().rowGroupBounds);
            } catch (error) {
              reportDiagnosticsError(
                "Failed to load the Parquet diagnostics snapshot.",
                error,
              );
            }
          })();
        } catch (error) {
          reportDiagnosticsError(
            "Parquet layer source diagnostics are unavailable.",
            error,
          );
        }

        const layerView = await mapElement.whenLayerView(layer);
        if (disposed) {
          return;
        }
        let layerViewUpdateVersion = 0;
        const refreshLayerViewCount = async () => {
          const updateVersion = layerViewUpdateVersion;
          try {
            const layerViewCount = await layerView.queryFeatureCount();
            if (!disposed && !layerView.updating && updateVersion === layerViewUpdateVersion) {
              setLayerViewFeatureCount(layerViewCount);
            }
          } catch (error) {
            if (!disposed) {
              console.error("Failed to query LayerView feature count.", error);
            }
          }
        };

        layerViewWatcher = reactiveUtils.watch(
          () => layerView.updating,
          (updating) => {
            layerViewUpdateVersion += 1;
            if (!updating) {
              void refreshLayerViewCount();
            }
          },
        );

        if (!layerView.updating) {
          void refreshLayerViewCount();
        }
      } catch (error) {
        if (!disposed) {
          console.error("Failed to query Parquet feature counts.", error);
        }
      }
    };
    void initializeLayer();

    return () => {
      disposed = true;
      rangeReadHandle?.remove();
      parquetSourceRef.current = null;
      layerViewWatcher?.remove();
      profileLayerCleanup?.();
      if (parquetLayerRef.current === layer) {
        parquetLayerRef.current = null;
      }
      setParquetLayer((currentLayer) => (currentLayer === layer ? null : currentLayer));
      map.layers.removeAll();
    };
  }, [activeDataset, activeProfile, downloadStore, mapReady]);

  useEffect(() => {
    const mapElement = mapElementRef.current;
    if (!mapElement || !parquetLayer || !activeProfile.mountMapComponents) {
      return;
    }

    return activeProfile.mountMapComponents({ mapElement, layer: parquetLayer });
  }, [activeProfile, parquetLayer]);

  useEffect(() => {
    const layer = parquetLayerRef.current;
    if (!layer) {
      return;
    }

    layer.labelsVisible = debugEnabled;
    layer.labelingInfo = debugEnabled
      ? [
          {
            minScale: 25_000,
            labelExpressionInfo: { expression: "Text($feature.geokey)" },
            labelPlacement: "always-horizontal",
            symbol: {
              type: "text",
              color: "#ef3573",
              haloColor: "white",
              haloSize: 1.5,
              font: { family: "Arial", size: 10 },
            },
          },
        ]
      : null;

    return () => {
      layer.labelsVisible = false;
      layer.labelingInfo = null;
    };
  }, [activeDataset.url, debugEnabled, mapReady]);

  useEffect(() => {
    const map = mapElementRef.current?.map;
    if (
      !mapReady ||
      !map ||
      !debugEnabled ||
      activeDataset.name !== censusDatasetName ||
      !rowGroupBounds
    ) {
      return;
    }

    const debugLayer = createRowGroupBoundsLayer(rowGroupBounds);
    map.add(debugLayer);

    return () => {
      map.remove(debugLayer);
    };
  }, [activeDataset.name, debugEnabled, mapReady, rowGroupBounds]);

  useEffect(() => {
    const view = mapElementRef.current?.view;
    if (!mapReady || !view) {
      setViewCenter(null);
      return;
    }

    return reactiveUtils
      .watch(
        () => view.center,
        (center) => {
          setViewCenter({
            latitude: center.latitude,
            longitude: center.longitude,
          });
        },
        { initial: true },
      )
      .remove;
  }, [mapReady]);

  return (
    <main
        ref={gridContainerRef}
        className={`grid-container${compactDetailsLayout ? " compact" : ""}${
          downloadDetailsEnabled ? "" : " details-disabled"
        }`}
      >
        <DatasetSelectionPanel
          activeDataset={activeDataset}
          byteSize={datasetByteSize}
          compression={compression}
          onDatasetSelect={selectDataset}
        />
        <calcite-panel className="grid-map">
          <calcite-label
            className="panel-metric"
            id="map-view-metrics"
            layout="inline"
            slot="header-actions-start"
          >
            Center
            <strong>
              {viewCenter
                ? `${viewCenter.longitude.toFixed(2)}, ${viewCenter.latitude.toFixed(2)}`
                : "…"}
            </strong>
            <span className="map-header-action-divider" aria-hidden="true">
              |
            </span>
            Features
            <strong>{formatFeatureCount(layerViewFeatureCount)}</strong>
          </calcite-label>
          <calcite-tooltip referenceElement="map-view-metrics">
            Current map center and feature count.
          </calcite-tooltip>
          <div
            className="map-profile-header-actions"
            slot="header-actions-end"
          >
            <div
              className="map-profile-header-action-target"
              ref={setMapHeaderActionsElement}
            />
            {activeDataset.bookmarks?.length ? (
              <>
                <calcite-button
                  ref={setBookmarksButton}
                  appearance="transparent"
                  iconStart="bookmark-f"
                  kind="neutral"
                  label="Bookmarks"
                  onClick={() => {
                    requestAnimationFrame(() =>
                      setBookmarksOpen((open) => !open),
                    );
                  }}
                />
                {bookmarksButton ? (
                  <calcite-popover
                    label="Bookmarks"
                    open={bookmarksOpen}
                    overlayPositioning="fixed"
                    placement="bottom-end"
                    referenceElement={bookmarksButton}
                    oncalcitePopoverClose={() => setBookmarksOpen(false)}
                  >
                    <div className="map-bookmark-list">
                      {activeDataset.bookmarks.map((bookmark) => (
                        <calcite-button
                          appearance="transparent"
                          key={bookmark.name}
                          kind="neutral"
                          width="full"
                          onClick={() => {
                            setBookmarksOpen(false);
                            void mapElementRef.current?.view.goTo(
                              new Viewpoint({
                                targetGeometry: {
                                  type: "point",
                                  longitude: bookmark.center[0],
                                  latitude: bookmark.center[1],
                                },
                                scale: bookmark.scale,
                              }),
                            );
                          }}
                        >
                          {bookmark.name}
                        </calcite-button>
                      ))}
                    </div>
                  </calcite-popover>
                ) : null}
              </>
            ) : null}
          </div>
          {downloadDetailsEnabled && compactDetailsLayout ? (
            <calcite-button
              appearance="transparent"
              aria-expanded={responsiveDetailsOpen}
              className="responsive-details-button"
              kind="neutral"
              label={responsiveDetailsOpen ? "Close details" : "Open details"}
              scale="m"
              slot="header-actions-end"
              onClick={() => setResponsiveDetailsOpen((open) => !open)}
            >
              Details
            </calcite-button>
          ) : null}
          <calcite-switch
            className="debug-switch"
            hidden
            slot="header-actions-end"
            label="Debug"
            labelTextEnd="Debug"
            checked={debugEnabled}
            oncalciteSwitchChange={(event: Event) =>
              setDebugEnabled((event.currentTarget as HTMLCalciteSwitchElement).checked)
            }
          />
          <MapCanvas
            dataset={activeDataset}
            headerActionsElement={mapHeaderActionsElement}
            layer={parquetLayer}
            mapElementRef={mapElementRef}
            profile={activeProfile}
          />
        </calcite-panel>

        {downloadDetailsEnabled && !compactDetailsLayout ? (
          <div className="grid-details-column">
            <div className="file-explorer-overview">
              <RowGroupOverviewMap
                basemapId={activeDataset.basemap}
                bounds={rowGroupBounds}
                center={activeDataset.center}
                key={activeDataset.id}
                mainMapElementRef={mapElementRef}
                scale={activeDataset.scale}
                spatialReferenceWkid={activeDataset.spatialReference}
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
            <FileDownload downloadStore={downloadStore} />
          </div>
        ) : null}
        {downloadDetailsEnabled && compactDetailsLayout && responsiveDetailsOpen ? (
          <aside
            className="responsive-details-overlay"
            aria-label="Details"
          >
            <div className="file-explorer-overview">
              <RowGroupOverviewMap
                basemapId={activeDataset.basemap}
                bounds={rowGroupBounds}
                center={activeDataset.center}
                key={activeDataset.id}
                mainMapElementRef={mapElementRef}
                scale={activeDataset.scale}
                spatialReferenceWkid={activeDataset.spatialReference}
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
            <FileDownload downloadStore={downloadStore} />
          </aside>
        ) : null}
        {fileStructureDialog ? (
          createPortal(
            <ParquetFileStructureDialog
              onClose={() => setFileStructureDialog(null)}
              snapshot={fileStructureDialog.snapshot}
              source={fileStructureDialog.source}
            />,
            document.body,
          )
        ) : null}
    </main>
  );
}

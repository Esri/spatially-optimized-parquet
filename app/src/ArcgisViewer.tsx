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
import Bookmark from "@arcgis/core/webmap/Bookmark";
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
import { DatasetSelectionMenu } from "./DatasetSelectionMenu";
import { formatByteSize } from "./formatByteSize";
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
const defaultColumnDownloadThreshold = 100 * 1024;
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
  mapElementRef,
  profile,
  layer,
}: {
  dataset: Dataset;
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
      <arcgis-zoom slot="top-left" />
      {MapSlotComponent ? (
        <MapSlotComponent key={layer?.id ?? "empty"} layer={layer} />
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
      <div className="row-group-overview-heading">Row Group Bounds</div>
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
            Loading row group bounds…
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
      <span className="column-download-share">
        {formatDownloadPercent(
          totalDownloadedByteLength === 0
            ? 0
            : (snapshot.downloadedByteLength / totalDownloadedByteLength) * 100,
        )}
      </span>
      <span className="column-range-content">
        <span className="column-label">
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

const FileDownloadStats = memo(function FileDownloadStats({
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
    <div className="file-stats-grid">
      <div className="file-stat">
        <div className="file-stat-label">Bytes Loaded</div>
        <div className="file-stat-value">
          {formatByteSize(summary.downloadedByteLength)}
          <span className="file-stat-unit">/ {byteLength === null ? "…" : formatByteSize(byteLength)}</span>
        </div>
      </div>
      <div className="file-stat">
        <div className="file-stat-label">Chunks Loaded</div>
        <div className="file-stat-value">
          {summary.cachedSubpartCount}
          <span className="file-stat-unit">/ {summary.subpartCount}</span>
        </div>
      </div>
    </div>
  );
});

const HiddenColumnCount = memo(function HiddenColumnCount({
  downloadStore,
}: {
  downloadStore: ParquetDownloadStore;
}) {
  const summary = useSyncExternalStore(
    downloadStore.subscribeSummary,
    downloadStore.getSummarySnapshot,
  );
  const hiddenColumnCount = summary.columnCount - summary.visibleColumnCount;

  if (hiddenColumnCount === 0) {
    return null;
  }

  return (
    <span className="occupancy-summary">
      {hiddenColumnCount} other {hiddenColumnCount === 1 ? "column" : "columns"} not downloaded
    </span>
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
        Show columns with at least
        <strong>{formatByteSize(minimumDownloadedByteLength)} downloaded</strong>
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
    <div className="file-download-content">
      <FileDownloadStats
        byteLength={topology.layout?.byteLength ?? null}
        downloadStore={downloadStore}
      />
      {topology.layout ? (
        <div className="column-threshold-frame">
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
                : "Loading Parquet diagnostics…"}
            </span>
          )}
        </div>
      </div>
      {topology.layout ? (
        <HiddenColumnCount downloadStore={downloadStore} />
      ) : null}
    </div>
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

const DownloadDetailsContent = memo(function DownloadDetailsContent({
  dataset,
  downloadStore,
  mainMapElementRef,
  rowGroupBounds,
  onOpenFileStructure,
}: {
  dataset: Dataset;
  downloadStore: ParquetDownloadStore;
  mainMapElementRef: React.RefObject<HTMLArcgisMapElement | null>;
  rowGroupBounds: readonly RowGroupBounds[] | null;
  onOpenFileStructure(): void;
}) {
  return (
    <div className="download-details-content">
      <RowGroupOverviewMap
        basemapId={dataset.basemap}
        bounds={rowGroupBounds}
        center={dataset.center}
        key={dataset.id}
        mainMapElementRef={mainMapElementRef}
        scale={dataset.scale}
        spatialReferenceWkid={dataset.spatialReference}
      />
      <calcite-button
        appearance="solid"
        className="file-structure-button"
        onClick={onOpenFileStructure}
        width="full"
      >
        Explore file structure
      </calcite-button>
      <FileDownload downloadStore={downloadStore} />
    </div>
  );
});

const DownloadDetailsPanel = memo(function DownloadDetailsPanel({
  className,
  dataset,
  downloadStore,
  mainMapElementRef,
  rowGroupBounds,
  onOpenFileStructure,
}: {
  className?: string;
  dataset: Dataset;
  downloadStore: ParquetDownloadStore;
  mainMapElementRef: React.RefObject<HTMLArcgisMapElement | null>;
  rowGroupBounds: readonly RowGroupBounds[] | null;
  onOpenFileStructure(): void;
}) {
  return (
    <calcite-panel className={className} heading="Details">
      <DownloadDetailsContent
        dataset={dataset}
        downloadStore={downloadStore}
        mainMapElementRef={mainMapElementRef}
        onOpenFileStructure={onOpenFileStructure}
        rowGroupBounds={rowGroupBounds}
      />
    </calcite-panel>
  );
});

export function ArcgisViewer() {
  const [datasetIndex, setDatasetIndex] = useState(0);
  const [mapReady, setMapReady] = useState(false);
  const [layerFeatureCount, setLayerFeatureCount] = useState<number | null>(null);
  const [layerViewFeatureCount, setLayerViewFeatureCount] = useState<number | null>(null);
  const [parquetLayer, setParquetLayer] = useState<ParquetLayer | null>(null);
  const [rowGroupBounds, setRowGroupBounds] =
    useState<readonly RowGroupBounds[] | null>(null);
  const [debugEnabled, setDebugEnabled] = useState(false);
  const [datasetDetailsOpen, setDatasetDetailsOpen] = useState(false);
  const [compactDetailsLayout, setCompactDetailsLayout] = useState(false);
  const [responsiveDetailsOpen, setResponsiveDetailsOpen] = useState(false);
  const [fileStructureDialog, setFileStructureDialog] = useState<{
    snapshot: FileStructureSnapshot;
    source: ArcgisParquetPageIndexSource;
  } | null>(null);
  const [datasetDetailsButton, setDatasetDetailsButton] =
    useState<HTMLCalciteButtonElement | null>(null);
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
  const datasetByteSize = datasetDetailsOpen
    ? downloadStore.getSnapshot().layout?.byteLength ?? activeDataset.byteSize
    : activeDataset.byteSize;
  const selectDataset = (index: number) => {
    setDatasetDetailsOpen(false);
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

    setLayerFeatureCount(null);
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

        const layerCount = await layer.queryFeatureCount();
        if (disposed) {
          return;
        }

        setLayerFeatureCount(layerCount);

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
    const mapElement = mapElementRef.current;
    if (!mapReady || !mapElement || !activeDataset.bookmarks?.length) {
      return;
    }

    const bookmarksElement = document.createElement("arcgis-bookmarks");
    bookmarksElement.bookmarks = activeDataset.bookmarks.map(
      (bookmark) =>
        new Bookmark({
          name: bookmark.name,
          viewpoint: new Viewpoint({
            targetGeometry: {
              type: "point",
              longitude: bookmark.center[0],
              latitude: bookmark.center[1],
            },
            scale: bookmark.scale,
          }),
        }),
    );
    const expandElement = document.createElement("arcgis-expand");
    expandElement.setAttribute("expand-icon", "bookmark");
    expandElement.setAttribute("expand-tooltip", "Bookmarks");
    expandElement.setAttribute("slot", "top-right");
    expandElement.append(bookmarksElement);
    mapElement.append(expandElement);

    return () => {
      expandElement.remove();
    };
  }, [activeDataset, mapReady]);

  return (
    <main
        ref={gridContainerRef}
        className={`grid-container${compactDetailsLayout ? " compact" : ""}${
          downloadDetailsEnabled ? "" : " details-disabled"
        }`}
      >
        <calcite-panel className="grid-map">
          <DatasetSelectionMenu
            activeDataset={activeDataset}
            onDatasetSelect={selectDataset}
          />
          <calcite-button
            ref={setDatasetDetailsButton}
            appearance="transparent"
            className="dataset-details-button"
            iconStart="information-f"
            kind="neutral"
            label={`Dataset details for ${activeDataset.name}`}
            scale="s"
            slot="header-actions-start"
            onClick={() => {
              requestAnimationFrame(() => setDatasetDetailsOpen((open) => !open));
            }}
          >
            <span className="dataset-details-label"></span>
          </calcite-button>
          {datasetDetailsButton ? (
            <calcite-popover
              className="dataset-details-popover"
              label="Dataset details"
              open={datasetDetailsOpen}
              placement="bottom-start"
              referenceElement={datasetDetailsButton}
              oncalcitePopoverClose={() => setDatasetDetailsOpen(false)}
            >
              <div className="dataset-details">
                <strong>{activeDataset.name}</strong>
                <dl>
                  <div>
                    <dt>Source</dt>
                    <dd>{activeDataset.source}</dd>
                  </div>
                  <div>
                    <dt>Source URL</dt>
                    <dd>
                      {activeDataset.sourceUrl ? (
                        <calcite-link
                          href={activeDataset.sourceUrl}
                          iconEnd="launch"
                          rel="noopener noreferrer"
                          target="_blank"
                        >
                          {activeDataset.sourceUrl}
                        </calcite-link>
                      ) : (
                        "Not provided"
                      )}
                    </dd>
                  </div>
                  <div>
                    <dt>Parquet URL</dt>
                    <dd>
                      <calcite-link
                        href={activeDataset.url}
                        iconEnd="launch"
                        rel="noopener noreferrer"
                        target="_blank"
                      >
                        {activeDataset.url}
                      </calcite-link>
                    </dd>
                  </div>
                  <div>
                    <dt>Byte count</dt>
                    <dd>{formatByteSize(datasetByteSize)}</dd>
                  </div>
                  <div>
                    <dt>Features</dt>
                    <dd>{activeDataset.count.toLocaleString()}</dd>
                  </div>
                </dl>
              </div>
            </calcite-popover>
          ) : null}
          <calcite-label
            className="panel-metric"
            id="layer-feature-count"
            slot="header-actions-end"
            layout="inline"
          >
            Layer
            <strong>{formatFeatureCount(layerFeatureCount)}</strong>
          </calcite-label>
          <calcite-tooltip referenceElement="layer-feature-count">
            Count of features in the dataset.
          </calcite-tooltip>
          <calcite-label
            className="panel-metric"
            id="layer-view-feature-count"
            slot="header-actions-end"
            layout="inline"
          >
            LayerView
            <strong>{formatFeatureCount(layerViewFeatureCount)}</strong>
          </calcite-label>
          <calcite-tooltip referenceElement="layer-view-feature-count">
            Count of features in the current view.
          </calcite-tooltip>
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
            layer={parquetLayer}
            mapElementRef={mapElementRef}
            profile={activeProfile}
          />
        </calcite-panel>

        {downloadDetailsEnabled && !compactDetailsLayout ? (
          <DownloadDetailsPanel
            className="grid-panel-desktop"
            dataset={activeDataset}
            downloadStore={downloadStore}
            mainMapElementRef={mapElementRef}
            onOpenFileStructure={openFileStructure}
            rowGroupBounds={rowGroupBounds}
          />
        ) : null}
        {downloadDetailsEnabled && compactDetailsLayout && responsiveDetailsOpen ? (
          <aside
            className="responsive-details-overlay"
            aria-label="Details"
          >
            <div className="responsive-details-content">
              <DownloadDetailsContent
                dataset={activeDataset}
                downloadStore={downloadStore}
                mainMapElementRef={mapElementRef}
                onOpenFileStructure={openFileStructure}
                rowGroupBounds={rowGroupBounds}
              />
            </div>
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

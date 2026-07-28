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
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";

import { datasets } from "./datasets";
import type { Dataset } from "./datasets";
import {
  resolveDatasetMapProfile,
  type DatasetEffectLayer,
  type DatasetMapProfile,
} from "./datasetMapProfiles";
import {
  ParquetDownloadStore,
  type BlockState,
  type DownloadBlock,
  type DownloadIndexDetail,
  type DownloadIndexBlock,
  type DownloadIndexSummary,
  type DownloadMetadataBlock,
} from "./parquetDownloadStore";
import {
  loadFileLayout,
  type RangeLifecycleObserver,
} from "./parquetFileLayout";
import { ParquetRequestInterceptor } from "./parquetRequestInterceptor";
import {
  calculateRowGroupFocusExtent,
  loadParquetRowGroupBounds,
  type RowGroupBounds,
} from "./parquetRowGroupBounds";

const defaultCenter: [number, number] = [-98, 39];
const defaultScale = 25_000_000;
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
  bounds: RowGroupBounds[],
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

interface HoveredBlock {
  element: HTMLSpanElement;
  fieldName: string;
  rowGroupIndex: number | null;
  start: number;
  end: number;
  state: string;
  indexDetails?: readonly DownloadIndexDetail[];
}

function summarizePageIndexColumns(details: readonly DownloadIndexDetail[]) {
  const summaryByField = new Map<
    string,
    { fieldName: string; downloadedByteLength: number; byteLength: number }
  >();

  for (const detail of details) {
    const summary = summaryByField.get(detail.fieldName) ?? {
      fieldName: detail.fieldName,
      downloadedByteLength: 0,
      byteLength: 0,
    };
    summary.downloadedByteLength += detail.downloadedByteLength;
    summary.byteLength += detail.byteLength;
    summaryByField.set(detail.fieldName, summary);
  }

  return Array.from(summaryByField.values());
}

function createDownloadTooltipStyle(element: HTMLSpanElement): CSSProperties {
  const bounds = element.getBoundingClientRect();
  const viewportPadding = 12;
  const tooltipHalfWidth = Math.min(210, (window.innerWidth - viewportPadding * 2) / 2);
  const horizontalCenter = bounds.left + bounds.width / 2;
  const left = Math.min(
    Math.max(horizontalCenter, viewportPadding + tooltipHalfWidth),
    window.innerWidth - viewportPadding - tooltipHalfWidth,
  );
  const placeBelow = bounds.top < window.innerHeight / 2;

  return {
    left,
    top: placeBelow ? bounds.bottom + 8 : bounds.top - 8,
    transform: placeBelow ? "translateX(-50%)" : "translate(-50%, -100%)",
  };
}

interface ChunkStyle extends CSSProperties {
  "--chunk-fill-background": string;
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
  dataset,
  mainMapElementRef,
}: {
  dataset: Dataset;
  mainMapElementRef: React.RefObject<HTMLArcgisMapElement | null>;
}) {
  const mapElementRef = useRef<HTMLArcgisMapElement>(null);
  const basemap = useMemo(
    () => createDatasetBasemap(dataset.basemap),
    [dataset.basemap],
  );
  const spatialReference = useMemo(
    () => createDatasetSpatialReference(dataset.spatialReference),
    [dataset.spatialReference],
  );

  useEffect(() => {
    const mapElement = mapElementRef.current;
    if (!mapElement || !dataset.url) {
      return;
    }

    mapElement.center = dataset.center;
    mapElement.scale = dataset.scale;

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
        const bounds = await loadParquetRowGroupBounds(dataset.url);
        if (disposed) {
          return;
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
  }, [dataset, mainMapElementRef]);

  return (
    <div className="row-group-overview">
      <div className="row-group-overview-heading">Row groups</div>
      <arcgis-map
        ref={mapElementRef}
        aria-label="Parquet row group overview"
        spatialReference={spatialReference}
        basemap={basemap}
        center={dataset.center}
        scale={dataset.scale}
      />
    </div>
  );
});

const DownloadChunk = memo(function DownloadChunk({
  block,
  fieldName,
  state,
  background,
  onHoverChange,
}: {
  block: DownloadBlock;
  fieldName: string;
  state: BlockState;
  background: string;
  onHoverChange: (hoveredBlock: HoveredBlock | null) => void;
}) {
  const style: ChunkStyle = { "--chunk-fill-background": background };

  return (
    <span
      className={`chunk ${state}`}
      onMouseEnter={(event) =>
        onHoverChange({
          element: event.currentTarget,
          fieldName,
          rowGroupIndex: block.rowGroupIndex,
          start: block.byteRange.start,
          end: block.byteRange.end,
          state,
        })
      }
      onMouseLeave={() => onHoverChange(null)}
      style={style}
    />
  );
});

const DownloadIndexChunk = memo(function DownloadIndexChunk({
  block,
  onHoverChange,
}: {
  block: DownloadIndexBlock;
  onHoverChange: (hoveredBlock: HoveredBlock | null) => void;
}) {
  const style: ChunkStyle = { "--chunk-fill-background": block.background };

  return (
    <span
      aria-label={`Page Index bytes: ${formatByteSize(block.downloadedByteLength)} of ${formatByteSize(block.byteLength)}`}
      className={`chunk ${block.state}`}
      onMouseEnter={(event) =>
        onHoverChange({
          element: event.currentTarget,
          fieldName: "Page Index",
          rowGroupIndex: null,
          start: 0,
          end: block.byteLength,
          state: block.state,
          indexDetails: block.details,
        })
      }
      onMouseLeave={() => onHoverChange(null)}
      style={style}
    />
  );
});

const IndexDownload = memo(function IndexDownload({
  summary,
  onHoverChange,
}: {
  summary: DownloadIndexSummary | null;
  onHoverChange: (hoveredBlock: HoveredBlock | null) => void;
}) {
  if (!summary) {
    return null;
  }

  return (
    <span className="column-range">
      <span className="column-label">
        Page Index
        <span className="column-progress">
          ({formatByteSize(summary.downloadedByteLength)}/{formatByteSize(summary.byteLength)})
        </span>
      </span>
      <span className="column-blocks">
        {summary.blocks.map((block) => (
          <DownloadIndexChunk block={block} key={block.id} onHoverChange={onHoverChange} />
        ))}
      </span>
    </span>
  );
});

const DownloadFooterChunk = memo(function DownloadFooterChunk({
  block,
  onHoverChange,
}: {
  block: DownloadMetadataBlock;
  onHoverChange: (hoveredBlock: HoveredBlock | null) => void;
}) {
  const style: ChunkStyle = { "--chunk-fill-background": block.background };

  return (
    <span
      className={`chunk ${block.state}`}
      onMouseEnter={(event) =>
        onHoverChange({
          element: event.currentTarget,
          fieldName: "Footer",
          rowGroupIndex: null,
          start: block.byteRange.start,
          end: block.byteRange.end,
          state: block.state,
        })
      }
      onMouseLeave={() => onHoverChange(null)}
      style={style}
    />
  );
});

const FooterDownload = memo(function FooterDownload({
  blocks,
  onHoverChange,
}: {
  blocks: readonly DownloadMetadataBlock[];
  onHoverChange: (hoveredBlock: HoveredBlock | null) => void;
}) {
  const footerByteLength = blocks.reduce(
    (total, block) => total + block.byteRange.end - block.byteRange.start,
    0,
  );

  return (
    <span className="column-range">
      <span className="column-label">
        Footer
        <span className="column-progress">
          ({formatByteSize(footerByteLength)})
        </span>
      </span>
      <span className="column-blocks">
        {blocks.map((block) => (
          <DownloadFooterChunk block={block} key={block.id} onHoverChange={onHoverChange} />
        ))}
      </span>
    </span>
  );
});

const FileDownload = memo(function FileDownload({
  downloadStore,
}: {
  downloadStore: ParquetDownloadStore;
}) {
  const [hoveredBlock, setHoveredBlock] = useState<HoveredBlock | null>(null);
  const downloadSnapshot = useSyncExternalStore(downloadStore.subscribe, downloadStore.getSnapshot);
  const { visibleColumns } = downloadSnapshot;
  const hiddenColumnCount = downloadSnapshot.fieldCount - visibleColumns.length;
  const tooltipStyle = hoveredBlock
    ? createDownloadTooltipStyle(hoveredBlock.element)
    : undefined;

  return (
    <calcite-block heading="File download" iconStart="grid" expanded collapsible>
      <div className="file-stats-grid">
        <div className="file-stat">
          <div className="file-stat-label">Bytes downloaded</div>
          <div className="file-stat-value">
            {formatByteSize(downloadSnapshot.downloadedByteLength)}
            <span className="file-stat-unit">
              / {downloadSnapshot.layout ? formatByteSize(downloadSnapshot.layout.byteLength) : "…"}
            </span>
          </div>
        </div>
        <div className="file-stat">
          <div className="file-stat-label">64 KiB chunks cached</div>
          <div className="file-stat-value">
            {downloadSnapshot.cachedSubchunkCount}
            <span className="file-stat-unit">/ {downloadSnapshot.subchunkCount}</span>
          </div>
        </div>
      </div>

      <div className="occupancy-grid-frame">
        <div
          className="occupancy-flow"
          role="img"
          aria-label="Parquet row groups, column chunks, and downloaded byte ranges"
        >
          {downloadSnapshot.layout ? (
            <>
              {hiddenColumnCount > 0 ? (
                <span className="occupancy-summary">
                  {hiddenColumnCount} other {hiddenColumnCount === 1 ? "column" : "columns"} not downloaded
                </span>
              ) : null}
              {visibleColumns.map((column) => (
                <span className="column-range" key={column.fieldName}>
                  <span className="column-label">
                    {column.fieldName}
                    <span className="column-progress">
                      ({formatByteSize(column.downloadedByteLength)}/
                      {formatByteSize(column.byteLength)})
                    </span>
                  </span>
                  <span className="column-blocks">
                    {column.blocks.map((block) => (
                      <DownloadChunk
                        background={
                          downloadSnapshot.blockDownloadBackgrounds.get(block.id) ??
                          "var(--calcite-color-foreground-3)"
                        }
                        block={block}
                        fieldName={column.fieldName}
                        key={block.id}
                        onHoverChange={setHoveredBlock}
                        state={downloadSnapshot.blockStates.get(block.id) ?? "empty"}
                      />
                    ))}
                  </span>
                </span>
              ))}
              <IndexDownload
                summary={downloadSnapshot.indexSummary}
                onHoverChange={setHoveredBlock}
              />
              <FooterDownload
                blocks={downloadSnapshot.footerBlocks}
                onHoverChange={setHoveredBlock}
              />
            </>
          ) : (
            <span className="occupancy-status">
              {downloadSnapshot.error
                ? "Unable to load Parquet footer metadata."
                : "Loading Parquet footer metadata…"}
            </span>
          )}
        </div>
      </div>
      {hoveredBlock ? (
        <div className="download-block-tooltip" role="tooltip" style={tooltipStyle}>
          {hoveredBlock.indexDetails ? (
            summarizePageIndexColumns(hoveredBlock.indexDetails).map((column) => (
              <div key={column.fieldName}>
                {column.fieldName} {formatByteSize(column.downloadedByteLength)}/
                {formatByteSize(column.byteLength)}
              </div>
            ))
          ) : (
            <>
              {hoveredBlock.rowGroupIndex !== null ? (
                <div>Row group: RG{hoveredBlock.rowGroupIndex}</div>
              ) : null}
              <div>Column: {hoveredBlock.fieldName}</div>
              <div>
                Byte range: {hoveredBlock.start}-{hoveredBlock.end - 1}
              </div>
              <div>Status: {hoveredBlock.state}</div>
            </>
          )}
        </div>
      ) : null}
    </calcite-block>
  );
});

const CompletedRequestCountChip = memo(function CompletedRequestCountChip({
  downloadStore,
}: {
  downloadStore: ParquetDownloadStore;
}) {
  const { completedRequestCount } = useSyncExternalStore(
    downloadStore.subscribe,
    downloadStore.getSnapshot,
  );

  return (
    <calcite-chip
      className="details-count-chip"
      slot="header-actions-end"
      scale="s"
      label={`Completed requests: ${completedRequestCount}`}
    >
      {completedRequestCount}
    </calcite-chip>
  );
});

function formatByteSize(byteSize: number): string {
  if (byteSize < 1024) {
    return `${byteSize} B`;
  }

  const unitIndex = Math.min(Math.floor(Math.log(byteSize) / Math.log(1024)), 3);
  const units = ["B", "KB", "MB", "GB"];
  const value = byteSize / 1024 ** unitIndex;

  return `${value >= 10 ? value.toFixed(0) : value.toFixed(1)} ${units[unitIndex]}`;
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
}: {
  dataset: Dataset;
  downloadStore: ParquetDownloadStore;
  mainMapElementRef: React.RefObject<HTMLArcgisMapElement | null>;
}) {
  return (
    <>
      <RowGroupOverviewMap
        dataset={dataset}
        mainMapElementRef={mainMapElementRef}
      />
      <FileDownload downloadStore={downloadStore} />
    </>
  );
});

const DownloadDetailsPanel = memo(function DownloadDetailsPanel({
  className,
  dataset,
  downloadStore,
  mainMapElementRef,
}: {
  className?: string;
  dataset: Dataset;
  downloadStore: ParquetDownloadStore;
  mainMapElementRef: React.RefObject<HTMLArcgisMapElement | null>;
}) {
  return (
    <calcite-panel className={className} heading="Details">
      <CompletedRequestCountChip downloadStore={downloadStore} />
      <DownloadDetailsContent
        dataset={dataset}
        downloadStore={downloadStore}
        mainMapElementRef={mainMapElementRef}
      />
    </calcite-panel>
  );
});

export function App() {
  const [datasetIndex, setDatasetIndex] = useState(0);
  const [mapReady, setMapReady] = useState(false);
  const [layerFeatureCount, setLayerFeatureCount] = useState<number | null>(null);
  const [layerViewFeatureCount, setLayerViewFeatureCount] = useState<number | null>(null);
  const [parquetLayer, setParquetLayer] = useState<ParquetLayer | null>(null);
  const [debugEnabled, setDebugEnabled] = useState(false);
  const [datasetDetailsOpen, setDatasetDetailsOpen] = useState(false);
  const [compactDetailsLayout, setCompactDetailsLayout] = useState(false);
  const [responsiveDetailsOpen, setResponsiveDetailsOpen] = useState(false);
  const [datasetDetailsButton, setDatasetDetailsButton] =
    useState<HTMLCalciteButtonElement | null>(null);
  const mapElementRef = useRef<HTMLArcgisMapElement>(null);
  const gridContainerRef = useRef<HTMLElement>(null);
  const parquetLayerRef = useRef<ParquetLayer | null>(null);
  const datasetMenuItemRef = useRef<HTMLCalciteMenuItemElement>(null);
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
    setDatasetIndex(index);
    setDatasetDetailsOpen(false);
    setResponsiveDetailsOpen(false);
    const datasetMenuItem = datasetMenuItemRef.current;
    if (datasetMenuItem) {
      datasetMenuItem.open = false;
    }
  };

  useLayoutEffect(() => {
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
    downloadStore.reset();
    const interceptor = new ParquetRequestInterceptor(activeDataset.url, downloadStore);
    interceptor.install();

    return () => {
      interceptor.dispose();
    };
  }, [activeDataset.url, downloadStore]);

  useEffect(() => {
    let active = true;
    const rangeObserver: RangeLifecycleObserver = {
      startRange: (range) => (active ? downloadStore.startRange(range) : ""),
      completeRange: (requestId) => {
        if (active && requestId) {
          downloadStore.completeRange(requestId);
        }
      },
      failRange: (requestId) => {
        if (active && requestId) {
          downloadStore.failRange(requestId);
        }
      },
    };

    const loadLayout = async () => {
      try {
        const layout = await loadFileLayout(activeDataset.url, rangeObserver);
        if (active) {
          downloadStore.setLayout(layout);
        }
      } catch (error) {
        if (active) {
          const layoutError = error instanceof Error ? error : new Error("Failed to load Parquet metadata.");
          downloadStore.setError(layoutError);
          console.error("Failed to load Parquet download layout.", error);
        }
      }
    };
    void loadLayout();

    return () => {
      active = false;
    };
  }, [activeDataset.url, downloadStore]);

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
    const profileLayerCleanup = hasDatasetEffectLayer(layer)
      ? activeProfile.configureLayer?.(layer)
      : undefined;
    const loadFeatureCounts = async () => {
      try {
        await layer.when();
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
    void loadFeatureCounts();

    return () => {
      disposed = true;
      layerViewWatcher?.remove();
      profileLayerCleanup?.();
      if (parquetLayerRef.current === layer) {
        parquetLayerRef.current = null;
      }
      setParquetLayer((currentLayer) => (currentLayer === layer ? null : currentLayer));
      map.layers.removeAll();
    };
  }, [activeDataset, activeProfile, mapReady]);

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
    if (!mapReady || !map || !debugEnabled || activeDataset.name !== censusDatasetName) {
      return;
    }

    let disposed = false;
    let debugLayer: FeatureLayer | undefined;

    const loadDebugLayer = async () => {
      try {
        const bounds = await loadParquetRowGroupBounds(activeDataset.url);
        if (disposed) {
          return;
        }

        debugLayer = createRowGroupBoundsLayer(bounds);
        map.add(debugLayer);
      } catch (error) {
        if (!disposed) {
          console.error("Failed to load Parquet row group bounds.", error);
        }
      }
    };

    void loadDebugLayer();

    return () => {
      disposed = true;
      if (debugLayer) {
        map.remove(debugLayer);
      }
    };
  }, [activeDataset.name, activeDataset.url, debugEnabled, mapReady]);

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
    <calcite-shell className="calcite-mode-dark">
      <calcite-navigation slot="header">
        <calcite-navigation-logo
          description="ArcGIS Maps SDK for JavaScript"
          heading="Spatially Optimized Parquet"
          slot="logo"
        />
        <calcite-menu slot="content-end" label="Application links">
          <calcite-menu-item
            text="Documentation"
            label="Documentation"
            iconStart="book"
          />
          <calcite-menu-item
            text="GitHub"
            label="Open GitHub"
            iconStart="launch"
            href="https://github.com"
            target="_blank"
            rel="noopener noreferrer"
          />
          <calcite-menu-item
            text="MapLibre Starter Code"
            label="MapLibre Starter Code"
            iconStart="rotate"
          />
        </calcite-menu>
      </calcite-navigation>

      <main
        ref={gridContainerRef}
        className={`grid-container${compactDetailsLayout ? " compact" : ""}`}
      >
        <calcite-panel className="grid-map">
          <calcite-menu
            className="dataset-menu"
            slot="header-actions-start"
            label="Dataset selection"
          >
            <calcite-menu-item
              ref={datasetMenuItemRef}
              text={`${activeDataset.name}`}
              label={`Selected dataset: ${activeDataset.name}`}
              iconStart="layers"
            >
              <div className="dataset-options" role="menu" slot="submenu-item">
                {datasets.map((dataset, index) => (
                  <div className="dataset-option" key={dataset.name} role="none">
                    <button
                      className="dataset-option-select"
                      disabled={!dataset.url}
                      onClick={() => selectDataset(index)}
                      role="menuitem"
                      type="button"
                    >
                      <span className="dataset-option-name">{dataset.name}</span>
                      <span className="dataset-option-metadata">
                        {formatByteSize(dataset.byteSize)} ·{" "}
                        {dataset.count.toLocaleString()} features
                      </span>
                    </button>
                    <span className="dataset-option-source">
                      {dataset.sourceUrl ? (
                        <calcite-link
                          href={dataset.sourceUrl}
                          iconEnd="launch"
                          rel="noopener noreferrer"
                          target="_blank"
                        >
                          {dataset.source}
                        </calcite-link>
                      ) : (
                        dataset.source
                      )}
                    </span>
                  </div>
                ))}
              </div>
            </calcite-menu-item>
          </calcite-menu>
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
          {compactDetailsLayout ? (
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

        {!compactDetailsLayout ? (
          <DownloadDetailsPanel
            className="grid-panel-desktop"
            dataset={activeDataset}
            downloadStore={downloadStore}
            mainMapElementRef={mapElementRef}
          />
        ) : null}
        {compactDetailsLayout && responsiveDetailsOpen ? (
          <aside
            className="responsive-details-overlay"
            aria-label="Details"
          >
            <div className="responsive-details-summary">
              <CompletedRequestCountChip downloadStore={downloadStore} />
            </div>
            <div className="responsive-details-content">
              <DownloadDetailsContent
                dataset={activeDataset}
                downloadStore={downloadStore}
                mainMapElementRef={mapElementRef}
              />
            </div>
          </aside>
        ) : null}
      </main>
    </calcite-shell>
  );
}

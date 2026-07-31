import ParquetLayer from "@arcgis/core/layers/ParquetLayer";
import VectorTileLayer from "@arcgis/core/layers/VectorTileLayer";
import SpatialReference from "@arcgis/core/geometry/SpatialReference";
import MapViewConstraints from "@arcgis/core/views/2d/MapViewConstraints";
import Viewpoint from "@arcgis/core/Viewpoint";
import Basemap from "@arcgis/core/Basemap";
import * as reactiveUtils from "@arcgis/core/core/reactiveUtils";
import {
  memo,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";

import { DatasetSelectionPanel } from "../common/dataset/DatasetSelectionPanel";
import { datasets, type Dataset } from "../common/dataset/datasets";
import { formatCompactCount } from "../common/formatCompactCount";
import { formatRatio } from "../common/formatNumber";
import { deriveFileDetailSummary } from "../parquet/fileDetails";
import {
  resolveDatasetMapProfile,
  type DatasetMapProfile,
} from "./profiles/profiles";
import { FileExplorer } from "./file-explorer/FileExplorer";
import { useArcgisDatasetSession } from "./useArcgisDatasetSession";
import styles from "./ArcgisViewer.module.css";

const defaultCenter: [number, number] = [-98, 39];
const defaultScale = 25_000_000;
const downloadDetailsEnabled = true;
const detailsLayoutBreakpoint = 1024;

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
  const ratio = formatRatio(
    summary.uncompressedSize,
    summary.compressedSize,
    1,
    "x",
  );
  return ratio ? `${codec} ${ratio}` : codec;
}

function createDatasetSpatialReference(wkid?: number): SpatialReference {
  return new SpatialReference({ wkid: wkid ?? 3857 });
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

function formatFeatureCount(featureCount: number | null): string {
  if (featureCount === null) {
    return "…";
  }

  return formatCompactCount(featureCount);
}

/**
 * Renders the ArcGIS dataset workspace and coordinates its map, dataset session, and download explorer.
 * This component owns viewer-level state so map integration and file diagnostics stay synchronized when the active dataset changes.
 */
export function ArcgisViewer() {
  const [datasetIndex, setDatasetIndex] = useState(0);
  const [mapReady, setMapReady] = useState(false);
  const [viewCenter, setViewCenter] = useState<{
    latitude: number;
    longitude: number;
  } | null>(null);
  const [debugEnabled, setDebugEnabled] = useState(false);
  const [compactDetailsLayout, setCompactDetailsLayout] = useState(false);
  const [responsiveDetailsOpen, setResponsiveDetailsOpen] = useState(false);
  const [mapHeaderActionsElement, setMapHeaderActionsElement] =
    useState<HTMLDivElement | null>(null);
  const [bookmarksButton, setBookmarksButton] =
    useState<HTMLCalciteButtonElement | null>(null);
  const [bookmarksOpen, setBookmarksOpen] = useState(false);
  const mapElementRef = useRef<HTMLArcgisMapElement>(null);
  const gridContainerRef = useRef<HTMLElement>(null);
  const activeDataset = datasets[datasetIndex];
  const activeProfile = resolveDatasetMapProfile(activeDataset.id);
  const {
    download: downloadSession,
    featureCount: layerViewFeatureCount,
    layer: parquetLayer,
    parquetSource,
    rowGroupBounds,
  } = useArcgisDatasetSession({
    dataset: activeDataset,
    mapElementRef,
    mapReady,
    profile: activeProfile,
  });
  const downloadTopology = useSyncExternalStore(
    downloadSession.topology.subscribe,
    downloadSession.topology.getSnapshot,
  );
  const datasetByteSize =
    downloadTopology.layout?.byteLength ?? activeDataset.byteSize;
  const fileLayout = downloadTopology.layout;
  const compression = fileLayout
    ? formatCompressionSummary(deriveFileDetailSummary(fileLayout))
    : null;
  const selectDataset = (index: number) => {
    setResponsiveDetailsOpen(false);
    if (index === datasetIndex) {
      return;
    }
    setDatasetIndex(index);
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
    if (!parquetLayer) {
      return;
    }

    parquetLayer.labelsVisible = debugEnabled;
    parquetLayer.labelingInfo = debugEnabled
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
      parquetLayer.labelsVisible = false;
      parquetLayer.labelingInfo = null;
    };
  }, [debugEnabled, parquetLayer]);

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
          if (
            center.latitude === null ||
            center.latitude === undefined ||
            center.longitude === null ||
            center.longitude === undefined
          ) {
            setViewCenter(null);
            return;
          }
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
      className={[
        styles.gridContainer,
        compactDetailsLayout ? styles.compact : null,
        downloadDetailsEnabled ? null : styles.detailsDisabled,
      ].filter(Boolean).join(" ")}
    >
        <DatasetSelectionPanel
          activeDataset={activeDataset}
          byteSize={datasetByteSize}
          compact={compactDetailsLayout}
          compression={compression}
          onDatasetSelect={selectDataset}
        />
        <calcite-panel className={styles.gridMap}>
          <calcite-label
            className={styles.panelMetric}
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
            <span className={styles.mapHeaderActionDivider} aria-hidden="true">
              |
            </span>
            Features
            <strong>{formatFeatureCount(layerViewFeatureCount)}</strong>
          </calcite-label>
          <calcite-tooltip referenceElement="map-view-metrics">
            Current map center and feature count.
          </calcite-tooltip>
          <div
            className={styles.mapProfileHeaderActions}
            slot="header-actions-end"
          >
            <div
              className={styles.mapProfileHeaderActionTarget}
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
                    <div className={styles.mapBookmarkList}>
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
            className={styles.debugSwitch}
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

        {downloadDetailsEnabled ? (
          <FileExplorer
            basemap={activeDataset.basemap}
            center={activeDataset.center}
            datasetId={activeDataset.id}
            downloadSession={downloadSession}
            layout={compactDetailsLayout ? "compact" : "desktop"}
            mainMapElementRef={mapElementRef}
            parquetSource={parquetSource}
            rowGroupBounds={rowGroupBounds}
            scale={activeDataset.scale}
            spatialReferenceWkid={activeDataset.spatialReference}
            visible={!compactDetailsLayout || responsiveDetailsOpen}
          />
        ) : null}
    </main>
  );
}

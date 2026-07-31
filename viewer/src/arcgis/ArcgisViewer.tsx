import Basemap from "@arcgis/core/Basemap";
import Viewpoint from "@arcgis/core/Viewpoint";
import * as reactiveUtils from "@arcgis/core/core/reactiveUtils";
import SpatialReference from "@arcgis/core/geometry/SpatialReference";
import ParquetLayer from "@arcgis/core/layers/ParquetLayer";
import VectorTileLayer from "@arcgis/core/layers/VectorTileLayer";
import MapViewConstraints from "@arcgis/core/views/2d/MapViewConstraints";
import {
  memo,
  type RefObject,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";

import { DatasetSelectionPanel } from "../common/dataset/DatasetSelectionPanel";
import { type Dataset, datasets } from "../common/dataset/datasets";
import { formatCompactCount } from "../common/formatCompactCount";
import { formatRatio } from "../common/formatNumber";
import { deriveFileDetailSummary } from "../parquet/fileDetails";
import styles from "./ArcgisViewer.module.css";
import { FileExplorer } from "./file-explorer/FileExplorer";
import {
  type DatasetMapProfile,
  resolveDatasetMapProfile,
} from "./profiles/profiles";
import { useArcgisDatasetSession } from "./useArcgisDatasetSession";

interface ViewCenter {
  latitude: number;
  longitude: number;
}

interface ResponsiveDetailsLayout {
  compact: boolean;
  open: boolean;
  setOpen(open: boolean): void;
}

interface ArcgisViewState {
  center: ViewCenter | null;
  ready: boolean;
}

interface MapPanelHeaderProps {
  compactDetailsLayout: boolean;
  dataset: Dataset;
  debugEnabled: boolean;
  detailsOpen: boolean;
  featureCount: number | null;
  mapElementRef: RefObject<HTMLArcgisMapElement | null>;
  setDebugEnabled(enabled: boolean): void;
  setDetailsOpen(open: boolean): void;
  setHeaderActionsElement(element: HTMLDivElement | null): void;
  viewCenter: ViewCenter | null;
}

interface MapCanvasProps {
  dataset: Dataset;
  headerActionsElement: HTMLElement | null;
  layer: ParquetLayer | null;
  mapElementRef: RefObject<HTMLArcgisMapElement | null>;
  profile: DatasetMapProfile;
}

interface ArcgisMapPanelProps {
  compactDetailsLayout: boolean;
  dataset: Dataset;
  detailsOpen: boolean;
  featureCount: number | null;
  layer: ParquetLayer | null;
  mapElementRef: RefObject<HTMLArcgisMapElement | null>;
  profile: DatasetMapProfile;
  setDetailsOpen(open: boolean): void;
  viewCenter: ViewCenter | null;
}

/**
 * Renders the ArcGIS dataset workspace and coordinates its map, dataset session, and download explorer.
 * This component owns viewer-level state so map integration and file diagnostics stay synchronized when the active dataset changes.
 */
export function ArcgisViewer() {
  const [datasetIndex, setDatasetIndex] = useState(0);
  const mapElementRef = useRef<HTMLArcgisMapElement>(null);
  const gridContainerRef = useRef<HTMLElement>(null);
  const {
    compact: compactDetailsLayout,
    open: responsiveDetailsOpen,
    setOpen: setResponsiveDetailsOpen,
  } = useResponsiveDetailsLayout(gridContainerRef);
  const { center: viewCenter, ready: mapReady } =
    useArcgisViewState(mapElementRef);
  const activeDataset = datasets[datasetIndex];
  const activeProfile = resolveDatasetMapProfile(activeDataset.id);
  const datasetSession = useArcgisDatasetSession({
    dataset: activeDataset,
    mapElementRef,
    mapReady,
    profile: activeProfile,
  });
  const downloadTopology = useSyncExternalStore(
    datasetSession.download.topology.subscribe,
    datasetSession.download.topology.getSnapshot,
  );
  const fileLayout = downloadTopology.layout;
  const datasetByteSize = fileLayout?.byteLength ?? activeDataset.byteSize;
  const compression = fileLayout
    ? formatCompressionSummary(deriveFileDetailSummary(fileLayout))
    : null;
  const selectDataset = (index: number) => {
    setResponsiveDetailsOpen(false);
    if (index !== datasetIndex) {
      setDatasetIndex(index);
    }
  };

  return (
    <main
      ref={gridContainerRef}
      className={[
        styles.gridContainer,
        compactDetailsLayout ? styles.compact : null,
      ].filter(Boolean).join(" ")}
    >
      <DatasetSelectionPanel
        activeDataset={activeDataset}
        byteSize={datasetByteSize}
        compact={compactDetailsLayout}
        compression={compression}
        onDatasetSelect={selectDataset}
      />
      <ArcgisMapPanel
        compactDetailsLayout={compactDetailsLayout}
        dataset={activeDataset}
        detailsOpen={responsiveDetailsOpen}
        featureCount={datasetSession.featureCount}
        layer={datasetSession.layer}
        mapElementRef={mapElementRef}
        profile={activeProfile}
        setDetailsOpen={setResponsiveDetailsOpen}
        viewCenter={viewCenter}
      />
      <FileExplorer
        dataset={activeDataset}
        layout={
          compactDetailsLayout
            ? { type: "compact", visible: responsiveDetailsOpen }
            : { type: "desktop" }
        }
        mapElementRef={mapElementRef}
        session={datasetSession}
      />
    </main>
  );
}

const defaultCenter: [number, number] = [-98, 39];
const defaultScale = 25_000_000;
const detailsLayoutBreakpoint = 1024;

function useResponsiveDetailsLayout(
  containerRef: RefObject<HTMLElement | null>,
): ResponsiveDetailsLayout {
  const [compact, setCompact] = useState(false);
  const [open, setOpen] = useState(false);

  useLayoutEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }

    const updateLayout = (width: number) => {
      const nextCompact = width <= detailsLayoutBreakpoint;
      setCompact(nextCompact);
      if (!nextCompact) {
        setOpen(false);
      }
    };
    updateLayout(container.getBoundingClientRect().width);

    const resizeObserver = new ResizeObserver(([entry]) => {
      updateLayout(entry.contentRect.width);
    });
    resizeObserver.observe(container);

    return () => resizeObserver.disconnect();
  }, [containerRef]);

  return { compact, open, setOpen };
}

function useArcgisViewState(
  mapElementRef: RefObject<HTMLArcgisMapElement | null>,
): ArcgisViewState {
  const [ready, setReady] = useState(false);
  const [center, setCenter] = useState<ViewCenter | null>(null);

  useEffect(() => {
    const mapElement = mapElementRef.current;
    if (!mapElement) {
      return;
    }

    const markReady = () => setReady(true);
    mapElement.addEventListener("arcgisViewReadyChange", markReady);
    if (mapElement.ready) {
      markReady();
    }

    return () => {
      mapElement.removeEventListener("arcgisViewReadyChange", markReady);
    };
  }, [mapElementRef]);

  useEffect(() => {
    const view = mapElementRef.current?.view;
    if (!ready || !view) {
      setCenter(null);
      return;
    }

    return reactiveUtils
      .watch(
        () => view.center,
        (nextCenter) => {
          if (nextCenter.latitude == null || nextCenter.longitude == null) {
            setCenter(null);
            return;
          }
          setCenter({
            latitude: nextCenter.latitude,
            longitude: nextCenter.longitude,
          });
        },
        { initial: true },
      )
      .remove;
  }, [mapElementRef, ready]);

  return { center, ready };
}

function useParquetDebugLabels(
  layer: ParquetLayer | null,
  enabled: boolean,
): void {
  useEffect(() => {
    if (!layer) {
      return;
    }

    layer.labelsVisible = enabled;
    layer.labelingInfo = enabled
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
  }, [enabled, layer]);
}

function ArcgisMapPanel({
  compactDetailsLayout,
  dataset,
  detailsOpen,
  featureCount,
  layer,
  mapElementRef,
  profile,
  setDetailsOpen,
  viewCenter,
}: ArcgisMapPanelProps) {
  const [debugEnabled, setDebugEnabled] = useState(false);
  const [headerActionsElement, setHeaderActionsElement] =
    useState<HTMLDivElement | null>(null);

  useParquetDebugLabels(layer, debugEnabled);

  return (
    <calcite-panel className={styles.gridMap}>
      <MapPanelHeader
        compactDetailsLayout={compactDetailsLayout}
        dataset={dataset}
        debugEnabled={debugEnabled}
        detailsOpen={detailsOpen}
        featureCount={featureCount}
        mapElementRef={mapElementRef}
        setDebugEnabled={setDebugEnabled}
        setDetailsOpen={setDetailsOpen}
        setHeaderActionsElement={setHeaderActionsElement}
        viewCenter={viewCenter}
      />
      <MapCanvas
        dataset={dataset}
        headerActionsElement={headerActionsElement}
        layer={layer}
        mapElementRef={mapElementRef}
        profile={profile}
      />
    </calcite-panel>
  );
}

function MapPanelHeader({
  compactDetailsLayout,
  dataset,
  debugEnabled,
  detailsOpen,
  featureCount,
  mapElementRef,
  setDebugEnabled,
  setDetailsOpen,
  setHeaderActionsElement,
  viewCenter,
}: MapPanelHeaderProps) {
  const [bookmarksButton, setBookmarksButton] =
    useState<HTMLCalciteButtonElement | null>(null);
  const [bookmarksOpen, setBookmarksOpen] = useState(false);

  return (
    <>
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
        <strong>{formatFeatureCount(featureCount)}</strong>
      </calcite-label>
      <calcite-tooltip referenceElement="map-view-metrics">
        Current map center and feature count.
      </calcite-tooltip>
      <div className={styles.mapProfileHeaderActions} slot="header-actions-end">
        <div
          className={styles.mapProfileHeaderActionTarget}
          ref={setHeaderActionsElement}
        />
        {dataset.bookmarks?.length ? (
          <>
            <calcite-button
              ref={setBookmarksButton}
              appearance="transparent"
              iconStart="bookmark-f"
              kind="neutral"
              label="Bookmarks"
              onClick={() => {
                requestAnimationFrame(() => {
                  setBookmarksOpen((open) => !open);
                });
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
                  {dataset.bookmarks.map((bookmark) => (
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
      {compactDetailsLayout ? (
        <calcite-button
          appearance="transparent"
          aria-expanded={detailsOpen}
          kind="neutral"
          label={detailsOpen ? "Close details" : "Open details"}
          scale="m"
          slot="header-actions-end"
          onClick={() => setDetailsOpen(!detailsOpen)}
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
        oncalciteSwitchChange={(event: Event) => {
          setDebugEnabled(
            (event.currentTarget as HTMLCalciteSwitchElement).checked,
          );
        }}
      />
    </>
  );
}

const MapCanvas = memo(function MapCanvas({
  dataset,
  headerActionsElement,
  layer,
  mapElementRef,
  profile,
}: MapCanvasProps) {
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

function formatFeatureCount(featureCount: number | null): string {
  return featureCount === null ? "…" : formatCompactCount(featureCount);
}

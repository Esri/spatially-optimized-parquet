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
import {
  createCustomUrlDataset,
  createPortalItemDataset,
  type Dataset,
  datasets,
} from "../common/dataset/datasets";
import { formatCompactCount } from "../common/formatCompactCount";
import { formatRatio } from "../common/formatNumber";
import { deriveFileDetailSummary } from "../parquet/fileDetails";
import styles from "./ArcgisViewer.module.css";
import { FileExplorer } from "./file-explorer/FileExplorer";
import {
  type DatasetMapProfile,
  resolveDatasetMapProfile,
} from "./profiles/profiles";
import {
  type ArcgisDatasetSessionResult,
  useArcgisDatasetSession,
} from "./useArcgisDatasetSession";

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
  dataset: Dataset;
  debugEnabled: boolean;
  detailsLayout: ResponsiveDetailsLayout;
  featureCount: number | null;
  mapElementRef: RefObject<HTMLArcgisMapElement | null>;
  setDebugEnabled(enabled: boolean): void;
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
  dataset: Dataset;
  detailsLayout: ResponsiveDetailsLayout;
  mapElementRef: RefObject<HTMLArcgisMapElement | null>;
  profile: DatasetMapProfile;
  session: ArcgisDatasetSessionResult;
  viewState: ArcgisViewState;
}

const defaultCenter: [number, number] = [-98, 39];
const defaultScale = 25_000_000;
const detailsLayoutBreakpoint = 1024;

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

/**
 * Renders the ArcGIS dataset workspace and coordinates its map, dataset session, and download explorer.
 * This component owns viewer-level state so map integration and file diagnostics stay synchronized when the active dataset changes.
 */
export function ArcgisViewer() {
  const [requestedDataset, setRequestedDataset] = useState<Dataset>(datasets[0]);
  const mapElementRef = useRef<HTMLArcgisMapElement>(null);
  const gridContainerRef = useRef<HTMLElement>(null);
  const detailsLayout = useResponsiveDetailsLayout(gridContainerRef);
  const viewState = useArcgisViewState(mapElementRef);
  const requestedProfile = resolveDatasetMapProfile(
    requestedDataset.kind === "preset" ? requestedDataset.id : undefined,
  );
  const datasetSession = useArcgisDatasetSession({
    dataset: requestedDataset,
    mapElementRef,
    mapReady: viewState.ready,
    profile: requestedProfile,
  });
  const activeDataset = datasetSession.dataset;
  const activeProfile = resolveDatasetMapProfile(
    activeDataset.kind === "preset" ? activeDataset.id : undefined,
  );
  const downloadTopology = useSyncExternalStore(
    datasetSession.download.topology.subscribe,
    datasetSession.download.topology.getSnapshot,
  );
  const fileLayout = downloadTopology.layout;
  const fileSummary = fileLayout ? deriveFileDetailSummary(fileLayout) : null;
  const requestDataset = (dataset: Dataset) => {
    detailsLayout.setOpen(false);
    setRequestedDataset(dataset);
  };

  return (
    <main
      ref={gridContainerRef}
      className={[
        styles.gridContainer,
        detailsLayout.compact ? styles.compact : null,
      ].filter(Boolean).join(" ")}
    >
      <DatasetSelectionPanel
        activeDataset={activeDataset}
        compact={detailsLayout.compact}
        customAction={{
          error: datasetSession.loadError?.message ?? null,
          loading: datasetSession.loading,
          onCustomUrlSubmit: (url) => {
            requestDataset(createCustomUrlDataset(url));
          },
          onPortalItemSubmit: (portalUrl, itemId) => {
            requestDataset(createPortalItemDataset(portalUrl, itemId));
          },
        }}
        metrics={{
          byteSize: fileLayout?.byteLength ?? null,
          compression: fileSummary
            ? formatCompressionSummary(fileSummary)
            : null,
          featureCount: fileSummary?.rowCount ?? null,
        }}
        onDatasetSelect={requestDataset}
        showPresetMetadata={false}
      />
      <ArcgisMapPanel
        dataset={activeDataset}
        detailsLayout={detailsLayout}
        mapElementRef={mapElementRef}
        profile={activeProfile}
        session={datasetSession}
        viewState={viewState}
      />
      <FileExplorer
        dataset={activeDataset}
        layout={
          detailsLayout.compact
            ? { type: "compact", visible: detailsLayout.open }
            : { type: "desktop" }
        }
        mapElementRef={mapElementRef}
        session={datasetSession}
      />
    </main>
  );
}

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

function ArcgisMapPanel({
  dataset,
  detailsLayout,
  mapElementRef,
  profile,
  session,
  viewState,
}: ArcgisMapPanelProps) {
  const [debugEnabled, setDebugEnabled] = useState(false);
  const [headerActionsElement, setHeaderActionsElement] =
    useState<HTMLDivElement | null>(null);

  useParquetDebugLabels(session.layer, debugEnabled);

  return (
    <calcite-panel className={styles.gridMap}>
      <MapPanelHeader
        dataset={dataset}
        debugEnabled={debugEnabled}
        detailsLayout={detailsLayout}
        featureCount={session.featureCount}
        mapElementRef={mapElementRef}
        setDebugEnabled={setDebugEnabled}
        setHeaderActionsElement={setHeaderActionsElement}
        viewCenter={viewState.center}
      />
      <MapCanvas
        dataset={dataset}
        headerActionsElement={headerActionsElement}
        layer={session.layer}
        mapElementRef={mapElementRef}
        profile={profile}
      />
    </calcite-panel>
  );
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

function MapPanelHeader({
  dataset,
  debugEnabled,
  detailsLayout,
  featureCount,
  mapElementRef,
  setDebugEnabled,
  setHeaderActionsElement,
  viewCenter,
}: MapPanelHeaderProps) {
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
        <MapBookmarksMenu dataset={dataset} mapElementRef={mapElementRef} />
      </div>
      {detailsLayout.compact ? (
        <calcite-button
          appearance="transparent"
          aria-expanded={detailsLayout.open}
          kind="neutral"
          label={detailsLayout.open ? "Close details" : "Open details"}
          scale="m"
          slot="header-actions-end"
          onClick={() => detailsLayout.setOpen(!detailsLayout.open)}
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

function MapBookmarksMenu({
  dataset,
  mapElementRef,
}: {
  dataset: Dataset;
  mapElementRef: RefObject<HTMLArcgisMapElement | null>;
}) {
  const [button, setButton] = useState<HTMLCalciteButtonElement | null>(null);
  const [open, setOpen] = useState(false);

  if (!dataset.bookmarks?.length) {
    return null;
  }

  return (
    <>
      <calcite-button
        ref={setButton}
        appearance="transparent"
        iconStart="bookmark-f"
        kind="neutral"
        label="Bookmarks"
        onClick={() => {
          requestAnimationFrame(() => setOpen((current) => !current));
        }}
      />
      {button ? (
        <calcite-popover
          label="Bookmarks"
          open={open}
          overlayPositioning="fixed"
          placement="bottom-end"
          referenceElement={button}
          oncalcitePopoverClose={() => setOpen(false)}
        >
          <div className={styles.mapBookmarkList}>
            {dataset.bookmarks.map((bookmark) => (
              <calcite-button
                appearance="transparent"
                key={bookmark.name}
                kind="neutral"
                width="full"
                onClick={() => {
                  setOpen(false);
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
  );
}

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

/**
 * The React view creates the MapLibre map and selects one SOP dataset.
 * `MaplibreViewer` converts the initial map scale to zoom and gives viewport reads to `MapLibreParquetLayer`.
 */
import maplibregl from "maplibre-gl";
import {
  type RefObject,
  useEffect,
  useRef,
  useState,
} from "react";

import "maplibre-gl/dist/maplibre-gl.css";

import { DatasetSelectionPanel } from "../common/dataset/DatasetSelectionPanel";
import {
  type PresetDataset,
  datasets,
} from "../common/dataset/datasets";
import { useDatasetRoute } from "../common/dataset/useDatasetRoute";
import { formatCompactCount } from "../common/formatCompactCount";
import {
  MapLibreParquetLayer,
} from "./MapLibreParquetLayer";
import type { DatasetLayerStatus } from "./interfaces";
import styles from "./MapLibreViewer.module.css";

const openFreeMapDarkStyleUrl = "https://tiles.openfreemap.org/styles/dark";
const mapScaleAtZoomZero = 295_829_355.4545656;

/**
 * `MaplibreViewer` owns the map and the selected dataset layer.
 * A dataset change removes the prior query before the new layer sets source data.
 */
export default function MaplibreViewer() {
  const containerRef = useRef<HTMLDivElement>(null);
  const { dataset: routedDataset, selectDataset } = useDatasetRoute();
  const activeDataset = routedDataset.kind === "preset"
    ? routedDataset
    : datasets[0];
  const map = useMapLibreMap(containerRef, datasets[0]);
  const status = useDatasetLayer(map, activeDataset);

  return (
    <main className={styles.maplibreWorkspace}>
      <DatasetSelectionPanel
        activeDataset={activeDataset}
        metrics={{
          byteSize: activeDataset.byteSize,
          compression: status.type === "ready" ? status.compression : null,
          featureCount: activeDataset.count,
        }}
        onDatasetSelect={(dataset) => {
          selectDataset(dataset);
        }}
      />
      <calcite-panel className={styles.maplibrePanel}>
        <DatasetStatusHeader status={status} />
        <div
          ref={containerRef}
          aria-label={`OpenFreeMap dark basemap with ${activeDataset.name}`}
          className={styles.maplibreMap}
        />
      </calcite-panel>
    </main>
  );
}

function DatasetStatusHeader({ status }: { status: DatasetLayerStatus }) {
  return (
    <>
      <calcite-label
        className={styles.panelMetric}
        layout="inline"
        slot="header-actions-start"
      >
        Features
        <strong>{formatFeatureCount(status)}</strong>
        <span className={styles.mapHeaderActionDivider} aria-hidden="true">
          |
        </span>
        LOD
        <strong>{status.type === "ready" ? status.lod : "…"}</strong>
      </calcite-label>
      {status.type === "loading" ? (
        <span className={styles.maplibreStatus} slot="header-actions-end">
          <calcite-loader inline label="Loading dataset" scale="s" />
          Loading...
        </span>
      ) : status.type === "failed" ? (
        <span className={styles.maplibreStatus} slot="header-actions-end">
          Dataset failed: {status.message}
        </span>
      ) : null}
    </>
  );
}

/** Create and dispose the MapLibre map for one container lifetime. */
function useMapLibreMap(
  containerRef: RefObject<HTMLDivElement | null>,
  initialDataset: PresetDataset,
): maplibregl.Map | null {
  const [map, setMap] = useState<maplibregl.Map | null>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }

    const nextMap = new maplibregl.Map({
      container,
      style: openFreeMapDarkStyleUrl,
      center: initialDataset.center,
      zoom: scaleToZoom(initialDataset.scale),
    });
    const resizeObserver = new ResizeObserver(() => nextMap.resize());
    resizeObserver.observe(container);
    setMap(nextMap);

    return () => {
      resizeObserver.disconnect();
      nextMap.remove();
      setMap((currentMap) => (currentMap === nextMap ? null : currentMap));
    };
  }, [containerRef, initialDataset]);

  return map;
}

/** Replace the dataset layer when the selected dataset changes. */
function useDatasetLayer(
  map: maplibregl.Map | null,
  dataset: PresetDataset,
): DatasetLayerStatus {
  const [status, setStatus] = useState<DatasetLayerStatus>({ type: "idle" });

  useEffect(() => {
    if (!map) {
      return;
    }

    setStatus({ type: "idle" });
    map.jumpTo({
      center: dataset.center,
      zoom: scaleToZoom(dataset.scale),
    });
    const layer = new MapLibreParquetLayer(map, dataset, {
      onStatusChange: setStatus,
    });
    layer.initialize();
    layer.refresh();

    return () => layer.dispose();
  }, [dataset, map]);

  return status;
}

/** Append `+` when the exact extent test reaches the feature limit. */
function formatFeatureCount(status: DatasetLayerStatus): string {
  if (status.type !== "ready") {
    return "…";
  }

  const count = formatCompactCount(status.featureCount);

  return status.featureLimitReached
    ? `${count}+`
    : count;
}

/**
 * Convert the map scale to zoom with the common zoom-zero scale.
 * Let `Query` choose the LOD from source resolution.
 */
function scaleToZoom(scale: number): number {
  return Math.log2(mapScaleAtZoomZero / scale);
}

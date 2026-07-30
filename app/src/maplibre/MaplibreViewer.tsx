import maplibregl from "maplibre-gl";
import { useEffect, useRef, useState } from "react";

import "maplibre-gl/dist/maplibre-gl.css";
import "./maplibre.css";

import { DatasetSelectionPanel } from "../DatasetSelectionPanel";
import { datasets } from "../datasets";
import {
  ParquetDatasetSource,
  type DatasetLayerStatus,
} from "./ParquetDatasetSource";

const openFreeMapDarkStyleUrl = "https://tiles.openfreemap.org/styles/dark";
const mapScaleAtZoomZero = 295_829_355.4545656;

export default function MaplibreViewer() {
  const containerRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<maplibregl.Map | null>(null);
  const datasetSourceRef = useRef<ParquetDatasetSource | null>(null);
  const [datasetIndex, setDatasetIndex] = useState(0);
  const [mapInstance, setMapInstance] = useState<maplibregl.Map | null>(null);
  const [status, setStatus] = useState<DatasetLayerStatus>({ type: "idle" });
  const activeDataset = datasets[datasetIndex];
  const initialDatasetRef = useRef(activeDataset);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) {
      return;
    }
    const initialDataset = initialDatasetRef.current;

    const map = new maplibregl.Map({
      container,
      style: openFreeMapDarkStyleUrl,
      center: initialDataset.center,
      zoom: scaleToZoom(initialDataset.scale),
    });
    const resizeObserver = new ResizeObserver(() => map.resize());
    resizeObserver.observe(container);
    mapRef.current = map;
    setMapInstance(map);

    return () => {
      datasetSourceRef.current?.dispose();
      datasetSourceRef.current = null;
      resizeObserver.disconnect();
      if (mapRef.current === map) {
        mapRef.current = null;
      }
      map.remove();
      setMapInstance((currentMap) => (currentMap === map ? null : currentMap));
    };
  }, []);

  useEffect(() => {
    if (!mapInstance || mapRef.current !== mapInstance) {
      return;
    }

    setStatus({ type: "idle" });
    mapInstance.jumpTo({
      center: activeDataset.center,
      zoom: scaleToZoom(activeDataset.scale),
    });
    const datasetSource = new ParquetDatasetSource(
      mapInstance,
      activeDataset,
      {
        onStatusChange: setStatus,
      },
    );
    datasetSourceRef.current = datasetSource;
    datasetSource.initialize();
    datasetSource.refresh();

    return () => {
      datasetSource.dispose();
      if (datasetSourceRef.current === datasetSource) {
        datasetSourceRef.current = null;
      }
    };
  }, [activeDataset, mapInstance]);

  return (
    <main className="maplibre-workspace">
      <DatasetSelectionPanel
        activeDataset={activeDataset}
        compression={
          status.type === "ready" ? status.compression : null
        }
        onDatasetSelect={setDatasetIndex}
      />
      <calcite-panel className="maplibre-panel">
        <calcite-label
          className="panel-metric"
          layout="inline"
          slot="header-actions-start"
        >
          Features
          <strong>{formatFeatureCount(status)}</strong>
          <span className="map-header-action-divider" aria-hidden="true">
            |
          </span>
          LOD
          <strong>{status.type === "ready" ? status.lod : "…"}</strong>
        </calcite-label>
        {status.type === "loading" ? (
          <span className="maplibre-status" slot="header-actions-end">
            <calcite-loader
              inline
              label="Loading dataset"
              scale="s"
            />
            Loading...
          </span>
        ) : status.type === "failed" ? (
          <span className="maplibre-status" slot="header-actions-end">
            Dataset failed: {status.message}
          </span>
        ) : null}
        <div
          ref={containerRef}
          aria-label={`OpenFreeMap dark basemap with ${activeDataset.name}`}
          className="maplibre-map"
        />
      </calcite-panel>
    </main>
  );
}

function formatFeatureCount(status: DatasetLayerStatus): string {
  if (status.type !== "ready") {
    return "…";
  }

  const count = new Intl.NumberFormat(undefined, {
    maximumFractionDigits: 1,
    notation: "compact",
  }).format(status.featureCount);

  return status.featureLimitReached
    ? `${count}+`
    : count;
}

function scaleToZoom(scale: number): number {
  return Math.log2(mapScaleAtZoomZero / scale);
}

import maplibregl from "maplibre-gl";
import { useEffect, useRef, useState } from "react";

import "maplibre-gl/dist/maplibre-gl.css";
import "./maplibre.css";

import { DatasetSelectionMenu } from "../DatasetSelectionMenu";
import { datasets } from "../datasets";
import {
  datasetFeatureLimit,
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
      <calcite-panel className="maplibre-panel">
        <DatasetSelectionMenu
          activeDataset={activeDataset}
          onDatasetSelect={setDatasetIndex}
        />
        <span className="maplibre-status" slot="header-actions-end">
          {formatStatus(status)}
        </span>
        <div
          ref={containerRef}
          aria-label={`OpenFreeMap dark basemap with ${activeDataset.name}`}
          className="maplibre-map"
        />
      </calcite-panel>
    </main>
  );
}

function formatStatus(status: DatasetLayerStatus): string {
  switch (status.type) {
    case "idle":
      return "Features: idle";
    case "loading":
      return "Features: loading...";
    case "ready":
      return `${status.featureCount.toLocaleString()} features · LOD ${status.lod}${
        status.featureLimitReached
          ? ` · limited to first ${datasetFeatureLimit.toLocaleString()} matches`
          : ""
      }`;
    case "failed":
      return `Dataset failed: ${status.message}`;
  }
}

function scaleToZoom(scale: number): number {
  return Math.log2(mapScaleAtZoomZero / scale);
}

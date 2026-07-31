import type {
  GeoJSONSource,
  Map as MapLibreMap,
  Source,
} from "maplibre-gl";

import type { Dataset } from "../common/dataset/datasets";
import type { Bounds, XZDisplayMetadata } from "./sop/metadata";
import {
  Query,
  type QueryResult,
} from "./sop/Query";
import type { GeoJSONFeatureCollection } from "./sop/geojson";

const parquetSourceId = "maplibre-parquet-dataset";
const parquetFillLayerId = "maplibre-parquet-fill";
const parquetOutlineLayerId = "maplibre-parquet-outline";
const parquetLineCasingLayerId = "maplibre-parquet-line-casing";
const parquetLineLayerId = "maplibre-parquet-line";

export type DatasetLayerStatus =
  | { type: "idle" }
  | { type: "loading" }
  | {
      type: "ready";
      featureCount: number;
      lod: number;
      featureLimitReached: boolean;
      compression: string | null;
    }
  | { type: "failed"; message: string };

interface MapLibreParquetLayerOptions {
  onStatusChange(status: DatasetLayerStatus): void;
  query?: Query;
}

const emptyFeatureCollection: GeoJSONFeatureCollection = {
  type: "FeatureCollection",
  features: [],
};

/**
 * Connects one SOP query to MapLibre sources and styled layers for the active dataset.
 * It owns map events, viewport extraction, request cancellation, query versioning, layer lifecycle, and status publication.
 */
export class MapLibreParquetLayer {
  private readonly _query: Query;
  private readonly _onStatusChange: MapLibreParquetLayerOptions["onStatusChange"];
  private _activeQuery: AbortController | null = null;
  private _queryVersion = 0;
  private _initialized = false;
  private _styleReady = false;
  private _disposed = false;

  constructor(
    private readonly _map: MapLibreMap,
    dataset: Dataset,
    options: MapLibreParquetLayerOptions,
  ) {
    this._query =
      options.query ?? new Query(dataset.url, dataset.byteSize);
    this._onStatusChange = options.onStatusChange;
  }

  initialize(): void {
    if (this._initialized || this._disposed) {
      return;
    }
    this._initialized = true;
    this._onStatusChange({ type: "idle" });
    this._map.on("moveend", this._handleMoveEnd);

    if (this._map.isStyleLoaded()) {
      this._styleReady = true;
    } else {
      this._map.on("load", this._handleMapLoad);
    }
  }

  refresh(): void {
    if (!this._initialized || !this._styleReady || this._disposed) {
      return;
    }

    void this._queryCurrentView();
  }

  dispose(): void {
    if (this._disposed) {
      return;
    }
    this._disposed = true;
    this._queryVersion += 1;
    this._activeQuery?.abort();
    this._activeQuery = null;
    this._map.off("load", this._handleMapLoad);
    this._map.off("moveend", this._handleMoveEnd);
    this._removeMapLayers();
    this._query.clear();
  }

  private readonly _handleMapLoad = (): void => {
    this._map.off("load", this._handleMapLoad);
    this._styleReady = true;
    this.refresh();
  };

  private readonly _handleMoveEnd = (): void => {
    this.refresh();
  };

  private async _queryCurrentView(): Promise<void> {
    const queryVersion = ++this._queryVersion;
    this._activeQuery?.abort();
    const controller = new AbortController();
    this._activeQuery = controller;
    this._onStatusChange({ type: "loading" });

    try {
      const metadata = await this._query.loadMetadata(controller.signal);
      this._ensureMapLayers(metadata.geometryType);
      const queryResult = await this._query.execute({
        queryExtents: getMapQueryExtents(
          this._map,
          metadata.fullExtent,
        ),
        sourceResolution: getMapSourceResolution(this._map),
        signal: controller.signal,
      });
      this._publish(queryVersion, controller.signal, queryResult);
    } catch (error) {
      if (!isAbortError(error)) {
        this._publishFailure(queryVersion, error);
      }
    } finally {
      if (queryVersion === this._queryVersion) {
        this._activeQuery = null;
      }
    }
  }

  private _publish(
    queryVersion: number,
    signal: AbortSignal,
    result: QueryResult,
  ): void {
    if (!this._isCurrentQuery(queryVersion, signal)) {
      return;
    }

    this._getMapSource().setData(result.featureCollection);
    this._onStatusChange({
      type: "ready",
      featureCount: result.featureCollection.features.length,
      lod: result.lod,
      featureLimitReached: result.featureLimitReached,
      compression: result.compression,
    });
  }

  private _publishFailure(queryVersion: number, error: unknown): void {
    if (queryVersion !== this._queryVersion) {
      return;
    }

    this._onStatusChange({
      type: "failed",
      message: error instanceof Error ? error.message : String(error),
    });
  }

  private _isCurrentQuery(
    queryVersion: number,
    signal: AbortSignal,
  ): boolean {
    return queryVersion === this._queryVersion && !signal.aborted;
  }

  private _ensureMapLayers(
    geometryType: XZDisplayMetadata["geometryType"],
  ): void {
    if (!this._map.getSource(parquetSourceId)) {
      this._map.addSource(parquetSourceId, {
        type: "geojson",
        data: emptyFeatureCollection,
      });
    }
    if (geometryType === "polyline") {
      this._addPolylineLayers();
    } else {
      this._addPolygonLayers();
    }
  }

  private _getMapSource(): GeoJSONSource {
    const source = this._map.getSource(parquetSourceId);
    if (!source || !isGeoJSONSource(source)) {
      throw new Error("MapLibre Parquet GeoJSON source is unavailable.");
    }

    return source;
  }

  private _addPolygonLayers(): void {
    if (!this._map.getLayer(parquetFillLayerId)) {
      this._map.addLayer({
        id: parquetFillLayerId,
        type: "fill",
        source: parquetSourceId,
        paint: {
          "fill-color": "#f28e2b",
          "fill-opacity": 0.42,
        },
      });
    }
    if (!this._map.getLayer(parquetOutlineLayerId)) {
      this._map.addLayer({
        id: parquetOutlineLayerId,
        type: "line",
        source: parquetSourceId,
        paint: {
          "line-color": "#ffffff",
          "line-opacity": 0.9,
          "line-width": 0.8,
        },
      });
    }
  }

  private _addPolylineLayers(): void {
    if (!this._map.getLayer(parquetLineCasingLayerId)) {
      this._map.addLayer({
        id: parquetLineCasingLayerId,
        type: "line",
        source: parquetSourceId,
        paint: {
          "line-color": "#ffffff",
          "line-opacity": 0.9,
          "line-width": 3,
        },
      });
    }
    if (!this._map.getLayer(parquetLineLayerId)) {
      this._map.addLayer({
        id: parquetLineLayerId,
        type: "line",
        source: parquetSourceId,
        paint: {
          "line-color": "#f28e2b",
          "line-opacity": 0.9,
          "line-width": 1.5,
        },
      });
    }
  }

  private _removeMapLayers(): void {
    if (!this._map.getStyle()) {
      return;
    }

    for (const layerId of [
      parquetOutlineLayerId,
      parquetFillLayerId,
      parquetLineLayerId,
      parquetLineCasingLayerId,
    ]) {
      if (this._map.getLayer(layerId)) {
        this._map.removeLayer(layerId);
      }
    }
    if (this._map.getSource(parquetSourceId)) {
      this._map.removeSource(parquetSourceId);
    }
  }
}

function getMapQueryExtents(
  map: MapLibreMap,
  fullExtent: Bounds,
): Bounds[] {
  const bounds = map.getBounds();
  const south = Math.max(bounds.getSouth(), fullExtent.ymin);
  const north = Math.min(bounds.getNorth(), fullExtent.ymax);
  if (south >= north) {
    return [];
  }

  const longitudeSpan = Math.min(
    Math.abs(bounds.getEast() - bounds.getWest()),
    360,
  );
  if (longitudeSpan >= 360) {
    return [{ ...fullExtent, ymin: south, ymax: north }];
  }

  const west = normalizeLongitude(bounds.getWest());
  const east = west + longitudeSpan;
  const longitudeExtents =
    east <= 180
      ? [{ xmin: west, xmax: east }]
      : [
          { xmin: west, xmax: 180 },
          { xmin: -180, xmax: east - 360 },
        ];

  return longitudeExtents.flatMap(({ xmin, xmax }) => {
    const clipped = {
      xmin: Math.max(xmin, fullExtent.xmin),
      xmax: Math.min(xmax, fullExtent.xmax),
      ymin: south,
      ymax: north,
    };

    return clipped.xmin < clipped.xmax ? [clipped] : [];
  });
}

function getMapSourceResolution(map: MapLibreMap): number {
  return 360 / (512 * 2 ** map.getZoom());
}

function normalizeLongitude(longitude: number): number {
  return ((((longitude + 180) % 360) + 360) % 360) - 180;
}

function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError";
}

function isGeoJSONSource(source: Source): source is GeoJSONSource {
  return source.type === "geojson";
}

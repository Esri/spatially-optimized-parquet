// Copyright 2026 Esri
//
// Licensed under the Apache License Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

/**
 * `MapLibreParquetLayer` converts MapLibre viewports to SOP query inputs.
 * The layer creates and removes the MapLibre source and style layers.
 * The layer sets GeoJSON source data only for the current request.
 */
import type {
  GeoJSONSource,
  Map as MapLibreMap,
  Source,
} from "maplibre-gl";

import type { PresetDataset } from "../common/dataset/datasets";
import type { DatasetLayerStatus } from "./interfaces";
import type { Bounds, XZDisplayMetadata } from "./sop/metadata";
import {
  Query,
  type QueryResult,
} from "./sop/Query";
import type { GeoJSONFeatureCollection } from "./sop/geojson";

interface MapLibreParquetLayerOptions {
  onStatusChange(status: DatasetLayerStatus): void;
  query?: Query;
}

const parquetSourceId = "maplibre-parquet-dataset";
const parquetFillLayerId = "maplibre-parquet-fill";
const parquetOutlineLayerId = "maplibre-parquet-outline";
const parquetLineCasingLayerId = "maplibre-parquet-line-casing";
const parquetLineLayerId = "maplibre-parquet-line";

/**
 * `MapLibreParquetLayer` owns one `Query` and all MapLibre state for a dataset.
 * The layer cancels old range reads and rejects late results.
 * `Query` reads metadata and each SOP column.
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
    dataset: PresetDataset,
    options: MapLibreParquetLayerOptions,
  ) {
    this._query =
      options.query ?? new Query(dataset.parquet.url, dataset.byteSize);
    this._onStatusChange = options.onStatusChange;
  }

  /**
   * Attach each map event listener once.
   * Call `refresh` after `initialize` when the style already exists.
   */
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

  /**
   * Start a query only after initialization and style load.
   * Replace the previous request with the new request.
   */
  refresh(): void {
    if (!this._initialized || !this._styleReady || this._disposed) {
      return;
    }

    void this._queryCurrentView();
  }

  /**
   * `dispose` prevents future source updates and releases all map state.
   * `dispose` cancels network reads.
   * A synchronous decode loop stops at its next signal check.
   */
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

  /**
   * Create one SOP query from the current map state.
   * Cancel asynchronous reads with the signal.
   * Reject late synchronous work with the request number.
   */
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
      this._applyQueryResult(queryVersion, controller.signal, queryResult);
    } catch (error) {
      if (!isAbortError(error)) {
        this._reportQueryFailure(queryVersion, error);
      }
    } finally {
      if (queryVersion === this._queryVersion) {
        this._activeQuery = null;
      }
    }
  }

  /**
   * Apply the query result only for the current request.
   */
  private _applyQueryResult(
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

  /** If no newer request exists, report the query error. */
  private _reportQueryFailure(queryVersion: number, error: unknown): void {
    if (queryVersion !== this._queryVersion) {
      return;
    }

    this._onStatusChange({
      type: "failed",
      message: error instanceof Error ? error.message : String(error),
    });
  }

  /**
   * Require the current request number and a live abort signal before a MapLibre change.
   */
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
        data: {
          type: "FeatureCollection",
          features: [],
        } satisfies GeoJSONFeatureCollection,
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

  /**
   * Remove each style layer before the source to preserve MapLibre dependency order.
   */
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

/**
 * `getMapQueryExtents` clips the WGS84 viewport to `fullExtent`.
 * Two ordered extents represent a viewport across the antimeridian.
 * The metadata reader accepts only WKID 4326, so the viewport uses degree values.
 */
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
    // A full-world viewport returns the dataset extent once.
    return [{ ...fullExtent, ymin: south, ymax: north }];
  }

  const west = normalizeLongitude(bounds.getWest());
  const east = west + longitudeSpan;
  // Split a viewport across the antimeridian into extents with `xmin` below `xmax`.
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

/** Convert MapLibre zoom to longitude units per source pixel for SOP LOD choice. */
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

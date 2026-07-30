import type {
  Feature,
  FeatureCollection,
  Position,
} from "geojson";
import type {
  GeoJSONSource,
  Map as MaplibreMap,
  Source,
} from "maplibre-gl";

import {
  loadDatasetParquetMetadata,
  selectLODLevel,
  type Bounds,
  type DatasetParquetMetadata,
  type LODLevel,
} from "./datasetParquetMetadata";
import {
  decodeGeometry,
  type SupportedGeometry,
} from "./pbf";
import {
  HyparquetLeafReader,
  type PhysicalColumn,
} from "./HyparquetLeafReader";
import {
  HttpParquetRangeReader,
  type ParquetRangeReader,
} from "./HttpParquetRangeReader";
import {
  getQueryXZRanges,
  mergeXZRanges,
  xzCodeMatches,
  type XZRange,
} from "./xz";
import type { Dataset } from "../common/dataset/datasets";

const parquetSourceId = "maplibre-parquet-dataset";
const parquetFillLayerId = "maplibre-parquet-fill";
const parquetOutlineLayerId = "maplibre-parquet-outline";
const parquetLineCasingLayerId = "maplibre-parquet-line-casing";
const parquetLineLayerId = "maplibre-parquet-line";
export const datasetFeatureLimit = 650_000;

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

interface ParquetDatasetSourceOptions {
  onStatusChange(status: DatasetLayerStatus): void;
  rangeReader?: ParquetRangeReader;
}

interface MatchingRowStore {
  count: number;
  featureLimitReached: boolean;
  rowsByGroup: Map<number, number[]>;
}

const emptyFeatureCollection: FeatureCollection = {
  type: "FeatureCollection",
  features: [],
};

/**
 * Coordinates viewport queries that turn Parquet row and geometry pages into a MapLibre GeoJSON source.
 * It owns map-layer setup, request cancellation, and query state so the React viewer only manages the source lifecycle.
 */
export class ParquetDatasetSource {
  private readonly _rangeReader: ParquetRangeReader;
  private readonly _onStatusChange: ParquetDatasetSourceOptions["onStatusChange"];
  private _metadata: DatasetParquetMetadata | null = null;
  private _leafReader: HyparquetLeafReader | null = null;
  private _activeQuery: AbortController | null = null;
  private _queryVersion = 0;
  private _initialized = false;
  private _styleReady = false;
  private _disposed = false;

  constructor(
    private readonly _map: MaplibreMap,
    private readonly _dataset: Dataset,
    options: ParquetDatasetSourceOptions,
  ) {
    this._rangeReader =
      options.rangeReader ??
      new HttpParquetRangeReader(_dataset.url, _dataset.byteSize);
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
    this._rangeReader.clear();
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
      const metadata = await this._getMetadata(controller.signal);
      this._ensureMapLayers();
      const queryExtents = getMapQueryExtents(
        this._map,
        metadata.display.fullExtent,
      );
      const xzRanges = mergeXZRanges(
        queryExtents.flatMap((extent) =>
          getQueryXZRanges(
            metadata.display.fullExtent,
            extent,
            metadata.display.maxLevel,
          ),
        ),
      );
      const lodLevel = selectLODLevel(
        metadata.display.levels,
        getMapSourceResolution(this._map),
      );
      const matchingRows = await this._queryMatchingRowIds(
        metadata,
        xzRanges,
        controller.signal,
      );

      const featureCollection = await this._buildFeatureCollection(
        metadata,
        lodLevel,
        matchingRows,
        queryExtents,
        controller.signal,
      );
      this._publish(
        queryVersion,
        controller.signal,
        featureCollection,
        lodLevel.level,
        matchingRows.featureLimitReached,
        metadata.compression,
      );
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

  private async _getMetadata(signal: AbortSignal): Promise<DatasetParquetMetadata> {
    if (this._metadata) {
      return this._metadata;
    }

    const metadata = await loadDatasetParquetMetadata(
      this._rangeReader.asAsyncBuffer(signal),
    );
    signal.throwIfAborted();
    this._metadata = metadata;
    this._leafReader = new HyparquetLeafReader(
      this._rangeReader,
      metadata.file,
    );
    return metadata;
  }

  private async _queryMatchingRowIds(
    metadata: DatasetParquetMetadata,
    xzRanges: XZRange[],
    signal: AbortSignal,
  ): Promise<MatchingRowStore> {
    const leafReader = this._requireLeafReader();
    const rowsByGroup = new Map<number, number[]>();
    let count = 0;
    const columns = leafReader.getColumnsMatchingXZ(
      metadata.display.codePath,
      xzRanges,
    );

    for (const column of columns) {
      const pages = await leafReader.selectPagesByXZ(
        column,
        xzRanges,
        signal,
      );
      const leafPages = await leafReader.readLeafPages<bigint>(column, pages, {
        signal,
      });
      const matchingRows: number[] = [];

      for (const page of leafPages) {
        for (let index = 0; index < page.values.length; index += 1) {
          if (xzCodeMatches(Number(page.values[index]), xzRanges)) {
            if (count === datasetFeatureLimit) {
              if (matchingRows.length > 0) {
                rowsByGroup.set(column.rowGroupIndex, matchingRows);
              }
              return {
                count,
                featureLimitReached: true,
                rowsByGroup,
              };
            }
            matchingRows.push(page.rowStart + index);
            count += 1;
          }
        }
      }

      if (matchingRows.length > 0) {
        rowsByGroup.set(column.rowGroupIndex, matchingRows);
      }
    }

    return { count, featureLimitReached: false, rowsByGroup };
  }

  private async _buildFeatureCollection(
    metadata: DatasetParquetMetadata,
    lodLevel: LODLevel,
    matchingRows: MatchingRowStore,
    queryExtents: Bounds[],
    signal: AbortSignal,
  ): Promise<FeatureCollection> {
    const features: Feature[] = [];
    const leafReader = this._requireLeafReader();

    for (const [rowGroupIndex, rowIds] of matchingRows.rowsByGroup) {
      signal.throwIfAborted();
      const geometryColumn = leafReader.getPhysicalColumn(
        rowGroupIndex,
        lodLevel.columnPath,
      );
      const geometryByRow = await this._readSelectedRows<Uint8Array | null>(
        geometryColumn,
        rowIds,
        true,
        signal,
      );

      for (const rowId of rowIds) {
        const bytes = geometryByRow.get(rowId);
        if (!(bytes instanceof Uint8Array) || bytes.length === 0) {
          continue;
        }

        const geometry = decodeGeometry(
          bytes,
          lodLevel.transform,
          metadata.display.geometryType,
        );
        if (!geometry || !geometryIntersectsExtents(geometry, queryExtents)) {
          continue;
        }

        features.push({
          type: "Feature",
          id: geometryColumn.rowGroupStart + rowId,
          properties: {},
          geometry,
        });
      }
    }

    return {
      type: "FeatureCollection",
      features,
    };
  }

  private async _readSelectedRows<T>(
    column: PhysicalColumn,
    rowIds: number[],
    binary: boolean,
    signal: AbortSignal,
  ): Promise<Map<number, T>> {
    const leafReader = this._requireLeafReader();
    const pages = await leafReader.selectPagesByRowId(column, rowIds, signal);
    const leafPages = await leafReader.readLeafPages<T>(column, pages, {
      signal,
      binary,
    });
    const requestedRows = new Set(rowIds);
    const valuesByRow = new Map<number, T>();

    for (const page of leafPages) {
      for (let index = 0; index < page.values.length; index += 1) {
        const rowId = page.rowStart + index;
        if (requestedRows.has(rowId)) {
          valuesByRow.set(rowId, page.values[index]);
        }
      }
    }

    return valuesByRow;
  }

  private _publish(
    queryVersion: number,
    signal: AbortSignal,
    featureCollection: FeatureCollection,
    lod: number,
    featureLimitReached: boolean,
    compression: string | null,
  ): void {
    if (!this._isCurrentQuery(queryVersion, signal)) {
      return;
    }

    this._getMapSource().setData(featureCollection);
    this._onStatusChange({
      type: "ready",
      featureCount: featureCollection.features.length,
      lod,
      featureLimitReached,
      compression,
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

  private _ensureMapLayers(): void {
    if (!this._map.getSource(parquetSourceId)) {
      this._map.addSource(parquetSourceId, {
        type: "geojson",
        data: emptyFeatureCollection,
      });
    }
    const geometryType = this._metadata?.display.geometryType;
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

  private _requireLeafReader(): HyparquetLeafReader {
    if (!this._leafReader) {
      throw new Error("Dataset Parquet metadata has not been initialized.");
    }

    return this._leafReader;
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
  map: MaplibreMap,
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

function getMapSourceResolution(map: MaplibreMap): number {
  return 360 / (512 * 2 ** map.getZoom());
}

function geometryIntersectsExtents(
  geometry: SupportedGeometry,
  extents: Bounds[],
): boolean {
  const bounds = geometryBounds(geometry);
  return extents.some(
    (extent) =>
      bounds.xmin <= extent.xmax &&
      bounds.xmax >= extent.xmin &&
      bounds.ymin <= extent.ymax &&
      bounds.ymax >= extent.ymin,
  );
}

function geometryBounds(geometry: SupportedGeometry): Bounds {
  const bounds: Bounds = {
    xmin: Number.POSITIVE_INFINITY,
    ymin: Number.POSITIVE_INFINITY,
    xmax: Number.NEGATIVE_INFINITY,
    ymax: Number.NEGATIVE_INFINITY,
  };
  const positions = getGeometryPositions(geometry);
  for (const [x, y] of positions) {
    bounds.xmin = Math.min(bounds.xmin, x);
    bounds.ymin = Math.min(bounds.ymin, y);
    bounds.xmax = Math.max(bounds.xmax, x);
    bounds.ymax = Math.max(bounds.ymax, y);
  }

  return bounds;
}

function getGeometryPositions(geometry: SupportedGeometry): Position[] {
  switch (geometry.type) {
    case "LineString":
      return geometry.coordinates;
    case "MultiLineString":
    case "Polygon":
      return geometry.coordinates.flat();
    case "MultiPolygon":
      return geometry.coordinates.flat(2);
    default: {
      const unsupported: never = geometry;
      return unsupported;
    }
  }
}

function normalizeLongitude(longitude: number): number {
  return ((((longitude + 180) % 360) + 360) % 360) - 180;
}

function isAbortError(error: unknown): boolean {
  return (
    error instanceof DOMException && error.name === "AbortError"
  );
}

function isGeoJSONSource(source: Source): source is GeoJSONSource {
  return source.type === "geojson";
}

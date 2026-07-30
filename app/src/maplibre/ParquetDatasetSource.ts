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
import { decodeEsriPbfGeometry } from "./pbf";
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

const emptyFeatureCollection: GeoJSON.FeatureCollection = {
  type: "FeatureCollection",
  features: [],
};

export class ParquetDatasetSource {
  private readonly rangeReader: ParquetRangeReader;
  private readonly onStatusChange: ParquetDatasetSourceOptions["onStatusChange"];
  private metadata: DatasetParquetMetadata | null = null;
  private leafReader: HyparquetLeafReader | null = null;
  private activeQuery: AbortController | null = null;
  private queryVersion = 0;
  private initialized = false;
  private styleReady = false;
  private disposed = false;

  constructor(
    private readonly map: MaplibreMap,
    private readonly dataset: Dataset,
    options: ParquetDatasetSourceOptions,
  ) {
    this.rangeReader =
      options.rangeReader ??
      new HttpParquetRangeReader(dataset.url, dataset.byteSize);
    this.onStatusChange = options.onStatusChange;
  }

  initialize(): void {
    if (this.initialized || this.disposed) {
      return;
    }
    this.initialized = true;
    this.onStatusChange({ type: "idle" });
    this.map.on("moveend", this.handleMoveEnd);

    if (this.map.isStyleLoaded()) {
      this.styleReady = true;
    } else {
      this.map.on("load", this.handleMapLoad);
    }
  }

  refresh(): void {
    if (!this.initialized || !this.styleReady || this.disposed) {
      return;
    }

    void this.queryCurrentView();
  }

  dispose(): void {
    if (this.disposed) {
      return;
    }
    this.disposed = true;
    this.queryVersion += 1;
    this.activeQuery?.abort();
    this.activeQuery = null;
    this.map.off("load", this.handleMapLoad);
    this.map.off("moveend", this.handleMoveEnd);
    this.removeMapLayers();
    this.rangeReader.clear();
  }

  private readonly handleMapLoad = (): void => {
    this.map.off("load", this.handleMapLoad);
    this.styleReady = true;
    this.refresh();
  };

  private readonly handleMoveEnd = (): void => {
    this.refresh();
  };

  private async queryCurrentView(): Promise<void> {
    const queryVersion = ++this.queryVersion;
    this.activeQuery?.abort();
    const controller = new AbortController();
    this.activeQuery = controller;
    this.onStatusChange({ type: "loading" });

    try {
      const metadata = await this.getMetadata(controller.signal);
      this.ensureMapLayers();
      const queryExtents = getMapQueryExtents(
        this.map,
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
        getMapSourceResolution(this.map),
      );
      const matchingRows = await this.queryMatchingRowIds(
        metadata,
        xzRanges,
        controller.signal,
      );

      const featureCollection = await this.buildFeatureCollection(
        metadata,
        lodLevel,
        matchingRows,
        queryExtents,
        controller.signal,
      );
      this.publish(
        queryVersion,
        controller.signal,
        featureCollection,
        lodLevel.level,
        matchingRows.featureLimitReached,
        metadata.compression,
      );
    } catch (error) {
      if (!isAbortError(error)) {
        this.publishFailure(queryVersion, error);
      }
    } finally {
      if (queryVersion === this.queryVersion) {
        this.activeQuery = null;
      }
    }
  }

  private async getMetadata(signal: AbortSignal): Promise<DatasetParquetMetadata> {
    if (this.metadata) {
      return this.metadata;
    }

    const metadata = await loadDatasetParquetMetadata(
      this.rangeReader.asAsyncBuffer(signal),
    );
    signal.throwIfAborted();
    this.metadata = metadata;
    this.leafReader = new HyparquetLeafReader(
      this.rangeReader,
      metadata.file,
    );
    return metadata;
  }

  private async queryMatchingRowIds(
    metadata: DatasetParquetMetadata,
    xzRanges: XZRange[],
    signal: AbortSignal,
  ): Promise<MatchingRowStore> {
    const leafReader = this.requireLeafReader();
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

  private async buildFeatureCollection(
    metadata: DatasetParquetMetadata,
    lodLevel: LODLevel,
    matchingRows: MatchingRowStore,
    queryExtents: Bounds[],
    signal: AbortSignal,
  ): Promise<GeoJSON.FeatureCollection> {
    const features: GeoJSON.Feature[] = [];
    const leafReader = this.requireLeafReader();

    for (const [rowGroupIndex, rowIds] of matchingRows.rowsByGroup) {
      signal.throwIfAborted();
      const geometryColumn = leafReader.getPhysicalColumn(
        rowGroupIndex,
        lodLevel.columnPath,
      );
      const geometryByRow = await this.readSelectedRows<Uint8Array | null>(
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

        const geometry = decodeEsriPbfGeometry(
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

  private async readSelectedRows<T>(
    column: PhysicalColumn,
    rowIds: number[],
    binary: boolean,
    signal: AbortSignal,
  ): Promise<Map<number, T>> {
    const leafReader = this.requireLeafReader();
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

  private publish(
    queryVersion: number,
    signal: AbortSignal,
    featureCollection: GeoJSON.FeatureCollection,
    lod: number,
    featureLimitReached: boolean,
    compression: string | null,
  ): void {
    if (!this.isCurrentQuery(queryVersion, signal)) {
      return;
    }

    this.getMapSource().setData(featureCollection);
    this.onStatusChange({
      type: "ready",
      featureCount: featureCollection.features.length,
      lod,
      featureLimitReached,
      compression,
    });
  }

  private publishFailure(queryVersion: number, error: unknown): void {
    if (queryVersion !== this.queryVersion) {
      return;
    }

    this.onStatusChange({
      type: "failed",
      message: error instanceof Error ? error.message : String(error),
    });
  }

  private isCurrentQuery(
    queryVersion: number,
    signal: AbortSignal,
  ): boolean {
    return queryVersion === this.queryVersion && !signal.aborted;
  }

  private ensureMapLayers(): void {
    if (!this.map.getSource(parquetSourceId)) {
      this.map.addSource(parquetSourceId, {
        type: "geojson",
        data: emptyFeatureCollection,
      });
    }
    const geometryType = this.metadata?.display.geometryType;
    if (geometryType === "polyline") {
      this.addPolylineLayers();
    } else {
      this.addPolygonLayers();
    }
  }

  private getMapSource(): GeoJSONSource {
    const source = this.map.getSource(parquetSourceId);
    if (!source || !isGeoJSONSource(source)) {
      throw new Error("MapLibre Parquet GeoJSON source is unavailable.");
    }

    return source;
  }

  private requireLeafReader(): HyparquetLeafReader {
    if (!this.leafReader) {
      throw new Error("Dataset Parquet metadata has not been initialized.");
    }

    return this.leafReader;
  }

  private addPolygonLayers(): void {
    if (!this.map.getLayer(parquetFillLayerId)) {
      this.map.addLayer({
        id: parquetFillLayerId,
        type: "fill",
        source: parquetSourceId,
        paint: {
          "fill-color": "#f28e2b",
          "fill-opacity": 0.42,
        },
      });
    }
    if (!this.map.getLayer(parquetOutlineLayerId)) {
      this.map.addLayer({
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

  private addPolylineLayers(): void {
    if (!this.map.getLayer(parquetLineCasingLayerId)) {
      this.map.addLayer({
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
    if (!this.map.getLayer(parquetLineLayerId)) {
      this.map.addLayer({
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

  private removeMapLayers(): void {
    if (!this.map.getStyle()) {
      return;
    }

    for (const layerId of [
      parquetOutlineLayerId,
      parquetFillLayerId,
      parquetLineLayerId,
      parquetLineCasingLayerId,
    ]) {
      if (this.map.getLayer(layerId)) {
        this.map.removeLayer(layerId);
      }
    }
    if (this.map.getSource(parquetSourceId)) {
      this.map.removeSource(parquetSourceId);
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
  geometry:
    | GeoJSON.Polygon
    | GeoJSON.MultiPolygon
    | GeoJSON.LineString
    | GeoJSON.MultiLineString,
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

function geometryBounds(
  geometry:
    | GeoJSON.Polygon
    | GeoJSON.MultiPolygon
    | GeoJSON.LineString
    | GeoJSON.MultiLineString,
): Bounds {
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

function getGeometryPositions(
  geometry:
    | GeoJSON.Polygon
    | GeoJSON.MultiPolygon
    | GeoJSON.LineString
    | GeoJSON.MultiLineString,
): GeoJSON.Position[] {
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

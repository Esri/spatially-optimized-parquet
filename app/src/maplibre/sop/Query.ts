import {
  loadDatasetParquetMetadata,
  selectLODLevel,
  type Bounds,
  type DatasetParquetMetadata,
  type LODLevel,
  type XZDisplayMetadata,
} from "./metadata";
import {
  type GeoJSONFeature,
  type GeoJSONFeatureCollection,
  type Position,
  type SupportedGeometry,
} from "./geojson";
import { decodeGeometry } from "./pbf";
import { PageReader, type PhysicalColumn } from "./PageReader";
import {
  RangeReader,
  type RangeReadable,
} from "./RangeReader";
import {
  getQueryXZRanges,
  mergeXZRanges,
  xzCodeMatches,
  type XZRange,
} from "./xz";

export const datasetFeatureLimit = 650_000;

export interface QueryInput {
  queryExtents: Bounds[];
  sourceResolution: number;
  signal: AbortSignal;
}

export interface QueryMetadata {
  fullExtent: Bounds;
  geometryType: XZDisplayMetadata["geometryType"];
}

export interface QueryResult {
  featureCollection: GeoJSONFeatureCollection;
  lod: number;
  featureLimitReached: boolean;
  compression: string | null;
  geometryType: XZDisplayMetadata["geometryType"];
}

interface QueryOptions {
  rangeReader?: RangeReadable;
}

interface MatchingRowStore {
  rowsByGroup: Map<number, number[]>;
}

interface FeatureCollectionResult {
  featureCollection: GeoJSONFeatureCollection;
  featureLimitReached: boolean;
}

/**
 * Executes SOP viewport queries against one remote Parquet dataset without depending on a map SDK.
 * It owns metadata caching, LOD and XZ selection, page reads, geometry decoding, filtering, and GeoJSON construction.
 */
export class Query {
  private readonly _rangeReader: RangeReadable;
  private _metadata: DatasetParquetMetadata | null = null;
  private _pageReader: PageReader | null = null;

  constructor(
    url: string,
    byteLength: number,
    options: QueryOptions = {},
  ) {
    this._rangeReader =
      options.rangeReader ?? new RangeReader(url, byteLength);
  }

  async loadMetadata(signal: AbortSignal): Promise<QueryMetadata> {
    const metadata = await this._getMetadata(signal);
    return {
      fullExtent: { ...metadata.display.fullExtent },
      geometryType: metadata.display.geometryType,
    };
  }

  async execute({
    queryExtents,
    sourceResolution,
    signal,
  }: QueryInput): Promise<QueryResult> {
    const metadata = await this._getMetadata(signal);
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
      sourceResolution,
    );
    const matchingRows = await this._queryMatchingRowIds(
      metadata,
      xzRanges,
      signal,
    );
    const {
      featureCollection,
      featureLimitReached,
    } = await this._buildFeatureCollection(
      lodLevel,
      matchingRows,
      queryExtents,
      metadata.display.geometryType,
      signal,
    );

    return {
      featureCollection,
      lod: lodLevel.level,
      featureLimitReached,
      compression: metadata.compression,
      geometryType: metadata.display.geometryType,
    };
  }

  clear(): void {
    this._rangeReader.clear();
  }

  private async _getMetadata(
    signal: AbortSignal,
  ): Promise<DatasetParquetMetadata> {
    if (this._metadata) {
      return this._metadata;
    }

    const metadata = await loadDatasetParquetMetadata(
      this._rangeReader.asAsyncBuffer(signal),
    );
    signal.throwIfAborted();
    this._metadata = metadata;
    this._pageReader = new PageReader(this._rangeReader, metadata.file);
    return metadata;
  }

  private async _queryMatchingRowIds(
    metadata: DatasetParquetMetadata,
    xzRanges: XZRange[],
    signal: AbortSignal,
  ): Promise<MatchingRowStore> {
    const pageReader = this._requirePageReader();
    const rowsByGroup = new Map<number, number[]>();
    const columns = pageReader.getColumnsMatchingXZ(
      metadata.display.codePath,
      xzRanges,
    );

    for (const column of columns) {
      const pages = await pageReader.selectPagesByXZ(
        column,
        xzRanges,
        signal,
      );
      const leafPages = await pageReader.readLeafPages<bigint>(
        column,
        pages,
        { signal },
      );
      const matchingRows: number[] = [];

      for (const page of leafPages) {
        for (let index = 0; index < page.values.length; index += 1) {
          if (xzCodeMatches(Number(page.values[index]), xzRanges)) {
            matchingRows.push(page.rowStart + index);
          }
        }
      }

      if (matchingRows.length > 0) {
        rowsByGroup.set(column.rowGroupIndex, matchingRows);
      }
    }

    return { rowsByGroup };
  }

  private async _buildFeatureCollection(
    lodLevel: LODLevel,
    matchingRows: MatchingRowStore,
    queryExtents: Bounds[],
    geometryType: XZDisplayMetadata["geometryType"],
    signal: AbortSignal,
  ): Promise<FeatureCollectionResult> {
    const features: GeoJSONFeature[] = [];
    const pageReader = this._requirePageReader();

    for (const [rowGroupIndex, rowIds] of matchingRows.rowsByGroup) {
      signal.throwIfAborted();
      const geometryColumn = pageReader.getPhysicalColumn(
        rowGroupIndex,
        lodLevel.columnPath,
      );
      const geometryByRow = await this._readSelectedRows<
        Uint8Array | null
      >(geometryColumn, rowIds, true, signal);

      for (const rowId of rowIds) {
        const bytes = geometryByRow.get(rowId);
        if (!(bytes instanceof Uint8Array) || bytes.length === 0) {
          continue;
        }

        const geometry = decodeGeometry(
          bytes,
          lodLevel.transform,
          geometryType,
        );
        if (
          !geometry ||
          !geometryIntersectsExtents(geometry, queryExtents)
        ) {
          continue;
        }

        if (features.length === datasetFeatureLimit) {
          return {
            featureCollection: {
              type: "FeatureCollection",
              features,
            },
            featureLimitReached: true,
          };
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
      featureCollection: {
        type: "FeatureCollection",
        features,
      },
      featureLimitReached: false,
    };
  }

  private async _readSelectedRows<T>(
    column: PhysicalColumn,
    rowIds: number[],
    binary: boolean,
    signal: AbortSignal,
  ): Promise<Map<number, T>> {
    const pageReader = this._requirePageReader();
    const pages = await pageReader.selectPagesByRowId(
      column,
      rowIds,
      signal,
    );
    const leafPages = await pageReader.readLeafPages<T>(column, pages, {
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

  private _requirePageReader(): PageReader {
    if (!this._pageReader) {
      throw new Error("Dataset Parquet metadata has not been initialized.");
    }

    return this._pageReader;
  }
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

/**
 * `Query` reads one SOP viewport from a remote Parquet file.
 * `Query` loads `geodisplay` metadata and finds rows through XZ and Parquet page indexes.
 * The PBF decoder reconstructs the selected LOD geometry.
 * Exact extent tests reject false XZ matches before `Query` returns GeoJSON.
 */
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

export interface QueryInput {
  /** Contains one clipped extent or two extents across the antimeridian. */
  queryExtents: Bounds[];
  /** Provides map units per source pixel for the SOP LOD choice. */
  sourceResolution: number;
  /** Stops range reads and later work at explicit signal checks. */
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

/** The limit applies after the exact geometry extent test. */
export const datasetFeatureLimit = 650_000;

/**
 * `Query` owns the metadata cache and `PageReader` for one Parquet file.
 * `Query` uses XZ ranges and page statistics to find candidate rows.
 * Synchronous row and geometry loops stop only at the next signal check.
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

  /**
   * Load metadata once before the first viewport query.
   * Return only the extent and geometry type that MapLibre needs.
   */
  async loadMetadata(signal: AbortSignal): Promise<QueryMetadata> {
    const metadata = await this._getMetadata(signal);
    return {
      fullExtent: { ...metadata.display.fullExtent },
      geometryType: metadata.display.geometryType,
    };
  }

  /**
   * `execute` prepares one LOD query from XZ ranges.
   * The query rejects false extent matches before the 650,000-feature limit applies.
   */
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

  /**
   * Clear completed byte ranges.
   * Keep parsed metadata and page index data for this `Query`.
   */
  clear(): void {
    this._rangeReader.clear();
  }

  /**
   * Validate root metadata on its first load.
   * Create `PageReader` after validation.
   */
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

  /**
   * Use XZ statistics to reject unrelated row groups and pages.
   * Test each decoded XZ code against the merged query ranges.
   */
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
            // Keep page order so later page selection receives sorted row IDs.
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

  /**
   * Read the selected LOD column only for candidate rows.
   * Test the exact extent of each decoded PBF geometry.
   * Do not treat an XZ match as proof of extent overlap.
   */
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

        // Apply the feature limit only after the exact geometry extent test.
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

  /**
   * Read only pages that contain requested row IDs.
   * Restore each row ID and value pair without a new sort.
   */
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

  /** Require metadata before any Parquet page read. */
  private _requirePageReader(): PageReader {
    if (!this._pageReader) {
      throw new Error("Dataset Parquet metadata has not been initialized.");
    }

    return this._pageReader;
  }
}

/**
 * Test exact geometry bounds against each clipped viewport extent.
 * Do not run a full geometry intersection.
 */
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

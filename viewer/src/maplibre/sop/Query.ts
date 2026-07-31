/**
 * `Query` reads one SOP viewport from a remote Parquet file.
 * `Query` delegates XZ page pruning and decoding to Hyparquet.
 * A narrow physical-leaf reader then fetches only the selected LOD for exact matching rows.
 * The PBF decoder reconstructs the selected LOD geometry.
 * Exact extent tests reject false XZ matches before `Query` returns GeoJSON.
 */
import {
  parquetRead,
  type ParquetQueryFilter,
  type SubColumnData,
} from "hyparquet";
import { compressors } from "hyparquet-compressors";

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
import {
  PhysicalLeafReader,
  type LeafRowReader,
} from "./PhysicalLeafReader";
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
  leafReader?: LeafRowReader;
  metadataLoader?: typeof loadDatasetParquetMetadata;
  parquetReader?: typeof parquetRead;
  rangeReader?: RangeReadable;
}

interface FeatureCollectionResult {
  featureCollection: GeoJSONFeatureCollection;
  featureLimitReached: boolean;
}

/** The limit applies after the exact geometry extent test. */
export const datasetFeatureLimit = 650_000;

/**
 * `Query` owns the metadata cache and SOP query semantics for one Parquet file.
 * Hyparquet uses XZ filters and page indexes to find candidate rows.
 * Synchronous row and geometry loops stop only at the next signal check.
 */
export class Query {
  private _leafReader: LeafRowReader | null;
  private readonly _metadataLoader: typeof loadDatasetParquetMetadata;
  private readonly _parquetReader: typeof parquetRead;
  private readonly _rangeReader: RangeReadable;
  private _metadata: DatasetParquetMetadata | null = null;

  constructor(
    url: string,
    byteLength: number,
    options: QueryOptions = {},
  ) {
    this._leafReader = options.leafReader ?? null;
    this._metadataLoader =
      options.metadataLoader ?? loadDatasetParquetMetadata;
    this._parquetReader = options.parquetReader ?? parquetRead;
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
    const rowIds =
      xzRanges.length === 0
        ? []
        : await this._queryMatchingRowIds(metadata, xzRanges, signal);
    signal.throwIfAborted();
    const geometryByRow =
      rowIds.length === 0
        ? new Map<number, Uint8Array | null>()
        : await this._requireLeafReader().readRows(
            lodLevel.columnPath,
            rowIds,
            signal,
          );
    const {
      featureCollection,
      featureLimitReached,
    } = await this._buildFeatureCollection(
      lodLevel,
      rowIds,
      geometryByRow,
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
   * Validate and cache root metadata on its first load.
   */
  private async _getMetadata(
    signal: AbortSignal,
  ): Promise<DatasetParquetMetadata> {
    if (this._metadata) {
      return this._metadata;
    }

    const metadata = await this._metadataLoader(
      this._rangeReader.asAsyncBuffer(signal),
    );
    signal.throwIfAborted();
    this._metadata = metadata;
    this._leafReader ??= new PhysicalLeafReader(
      this._rangeReader,
      metadata.file,
    );
    return metadata;
  }

  /**
   * Let Hyparquet prune XZ pages, then inspect decoded XZ values for exact row IDs.
   * Avoid object assembly because `onPage` exposes physical values and global row offsets.
   */
  private async _queryMatchingRowIds(
    metadata: DatasetParquetMetadata,
    xzRanges: XZRange[],
    signal: AbortSignal,
  ): Promise<number[]> {
    const rowIds: number[] = [];
    const codePath = metadata.display.codePath;

    await this._parquetReader({
      file: this._rangeReader.asAsyncBuffer(signal),
      metadata: metadata.file,
      columns: [getTopLevelColumn(codePath)],
      filter: createXZFilter(codePath, xzRanges),
      rowFormat: "object",
      usePageIndex: true,
      compressors,
      onPage: (page) => {
        if (!pathsEqual(page.pathInSchema, codePath)) {
          return;
        }
        appendMatchingRowIds(rowIds, page, xzRanges);
      },
    });

    return [...new Set(rowIds)].sort((left, right) => left - right);
  }

  /**
   * Decode selected LOD values for exact matching rows.
   * Test the exact extent of each decoded PBF geometry.
   * Do not treat an XZ match as proof of extent overlap.
   */
  private async _buildFeatureCollection(
    lodLevel: LODLevel,
    rowIds: number[],
    geometryByRow: Map<number, unknown>,
    queryExtents: Bounds[],
    geometryType: XZDisplayMetadata["geometryType"],
    signal: AbortSignal,
  ): Promise<FeatureCollectionResult> {
    const features: GeoJSONFeature[] = [];
    for (const rowId of rowIds) {
      signal.throwIfAborted();
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
        properties: {},
        geometry,
      });
    }

    return {
      featureCollection: {
        type: "FeatureCollection",
        features,
      },
      featureLimitReached: false,
    };
  }

  private _requireLeafReader(): LeafRowReader {
    if (!this._leafReader) {
      throw new Error("Dataset Parquet metadata has not been initialized.");
    }

    return this._leafReader;
  }
}

/** Convert inclusive XZ intervals into Hyparquet range predicates. */
export function createXZFilter(
  codePath: readonly string[],
  ranges: XZRange[],
): ParquetQueryFilter {
  if (ranges.length === 0) {
    throw new Error("XZ filter requires at least one range.");
  }

  const field = codePath.join(".");
  const filters = ranges.map(({ start, end }) => ({
    [field]: {
      $gte: start,
      $lte: end,
    },
  }));

  return filters.length === 1 ? filters[0] : { $or: filters };
}

/** Project the top-level column that owns a physical metadata path. */
export function getTopLevelColumn(path: readonly string[]): string {
  const [column] = path;
  if (!column) {
    throw new Error("Parquet column path must not be empty.");
  }

  return column;
}

function appendMatchingRowIds(
  rowIds: number[],
  page: SubColumnData,
  ranges: XZRange[],
): void {
  const values = page.columnData as ArrayLike<unknown>;
  for (let index = 0; index < values.length; index += 1) {
    if (xzCodeMatches(Number(values[index]), ranges)) {
      rowIds.push(page.rowStart + index);
    }
  }
}

function pathsEqual(
  left: readonly string[],
  right: readonly string[],
): boolean {
  return (
    left.length === right.length &&
    left.every((part, index) => part === right[index])
  );
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

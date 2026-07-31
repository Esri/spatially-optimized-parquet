/**
 * The metadata reader reads the root Parquet `geodisplay` entry.
 * The reader validates XZ metadata, LOD columns, transforms, and physical column paths.
 * The MapLibre example accepts only WGS84 XY polygon or polyline Esri PBF.
 * The reader does not use Spark field metadata.
 */
import {
  parquetMetadataAsync,
  type AsyncBuffer,
  type FileMetaData,
} from "hyparquet";

import { formatRatio } from "../../common/formatNumber";

export interface Bounds {
  xmin: number;
  ymin: number;
  xmax: number;
  ymax: number;
}

export interface QuantizationTransform {
  scale: [number, number, number, number];
  translate: [number, number, number, number];
}

export interface LODLevel {
  columnPath: string[];
  level: number;
  resolution: number;
  transform: QuantizationTransform;
}

export interface XZDisplayMetadata {
  codePath: string[];
  fullExtent: Bounds;
  geometryType: "polygon" | "polyline";
  maxLevel: number;
  levels: LODLevel[];
  version: string | null;
}

export interface DatasetParquetMetadata {
  compression: string | null;
  file: FileMetaData;
  display: XZDisplayMetadata;
}

const maximumResolutionDrift = 2 ** 0.001;

/**
 * Parse the root SOP `geodisplay` entry from the Parquet footer.
 * Before any viewport query, validate every referenced physical column.
 */
export async function loadDatasetParquetMetadata(
  file: AsyncBuffer,
): Promise<DatasetParquetMetadata> {
  const metadata = await parquetMetadataAsync(file, { geoparquet: false });
  const displayValue = metadata.key_value_metadata?.find(
    ({ key }) => key === "geodisplay",
  )?.value;

  if (!displayValue) {
    throw new Error("Parquet dataset does not define geodisplay metadata.");
  }

  const display = parseXZDisplayMetadata(JSON.parse(displayValue));
  validatePhysicalColumn(metadata, display.codePath);
  for (const level of display.levels) {
    validatePhysicalColumn(metadata, level.columnPath);
  }

  return {
    compression: formatCompression(metadata),
    file: metadata,
    display,
  };
}

/**
 * Report the physical codec and the ratio of total raw bytes to compressed bytes.
 */
function formatCompression(metadata: FileMetaData): string | null {
  const codecs = new Set<string>();
  let compressedSize = 0n;
  let uncompressedSize = 0n;

  for (const rowGroup of metadata.row_groups) {
    for (const column of rowGroup.columns) {
      const columnMetadata = column.meta_data;
      if (!columnMetadata) {
        continue;
      }
      codecs.add(columnMetadata.codec);
      compressedSize += columnMetadata.total_compressed_size;
      uncompressedSize += columnMetadata.total_uncompressed_size;
    }
  }

  if (codecs.size === 0) {
    return null;
  }
  const codec = codecs.size === 1 ? [...codecs][0] : "Mixed";
  const ratio = formatRatio(
    Number(uncompressedSize),
    Number(compressedSize),
    1,
    "x",
  );
  return ratio ? `${codec.toUpperCase()} ${ratio}` : codec;
}

/**
 * Use `spec/display-optimization.md#select-multiscale-levels` to choose the closest stored level with enough detail.
 * If numeric drift prevents a match, use the most detailed level.
 */
export function selectLODLevel(
  levels: LODLevel[],
  sourceResolution: number,
): LODLevel {
  if (!Number.isFinite(sourceResolution) || sourceResolution <= 0) {
    throw new Error("Map source resolution must be a positive finite number.");
  }

  const level = levels.find(
    (candidate) =>
      candidate.resolution <= sourceResolution * maximumResolutionDrift,
  );

  return level ?? levels.at(-1)!;
}

/**
 * Require root metadata and WGS84 coordinates.
 * Accept only XY polygon or polyline Esri PBF.
 */
export function parseXZDisplayMetadata(value: unknown): XZDisplayMetadata {
  if (!isRecord(value)) {
    throw new Error("Geodisplay metadata must contain an object.");
  }
  if (value.type !== "xz") {
    throw new Error(`Expected XZ geodisplay metadata, found ${String(value.type)}.`);
  }
  if (value.encoding !== "esriPBF") {
    throw new Error(
      `Expected esriPBF display geometry, found ${String(value.encoding)}.`,
    );
  }
  if (value.geometryType !== "polygon" && value.geometryType !== "polyline") {
    throw new Error(
      `Expected polygon or polyline display geometry, found ${String(value.geometryType)}.`,
    );
  }
  if (value.wkid !== 4326) {
    throw new Error(`Expected WKID 4326, found ${String(value.wkid)}.`);
  }
  if (value.hasZ === true || value.hasM === true) {
    throw new Error("The MapLibre Parquet POC supports XY display geometry only.");
  }

  const codePath = normalizeColumnPath(value.code, "geodisplay.code");
  const maxLevel = requireInteger(value.maxLevel, "geodisplay.maxLevel");
  const fullExtent = parseBounds(value.fullExtent);
  if (!Array.isArray(value.levels) || value.levels.length === 0) {
    throw new Error("Geodisplay metadata must define at least one LOD level.");
  }

  const levels = value.levels
    .map(parseLODLevel)
    // Sort levels from coarse to detailed so LOD choice follows specification order.
    .sort((left, right) => left.level - right.level);

  return {
    codePath,
    fullExtent,
    geometryType: value.geometryType,
    maxLevel,
    levels,
    version: typeof value.version === "string" ? value.version : null,
  };
}

/**
 * Convert one SOP `MultiscaleLevel` entry to its physical column path and quantization transform.
 */
function parseLODLevel(value: unknown): LODLevel {
  if (!isRecord(value)) {
    throw new Error("Each geodisplay LOD level must contain an object.");
  }
  return {
    columnPath: normalizeColumnPath(
      value.column,
      "geodisplay.levels[].column",
    ),
    level: requireInteger(value.level, "geodisplay.levels[].level"),
    resolution: requirePositiveNumber(
      value.resolution,
      "geodisplay.levels[].resolution",
    ),
    transform: parseTransform(value.transform),
  };
}

/**
 * Require `fullExtent` to have finite values and positive area.
 * The query uses the validated extent for XZ codes and viewport limits.
 */
function parseBounds(value: unknown): Bounds {
  if (!isRecord(value)) {
    throw new Error("Geodisplay fullExtent must contain an object.");
  }

  const bounds = {
    xmin: requireNumber(value.xmin, "fullExtent.xmin"),
    ymin: requireNumber(value.ymin, "fullExtent.ymin"),
    xmax: requireNumber(value.xmax, "fullExtent.xmax"),
    ymax: requireNumber(value.ymax, "fullExtent.ymax"),
  };
  if (bounds.xmin >= bounds.xmax || bounds.ymin >= bounds.ymax) {
    throw new Error("Geodisplay fullExtent must have positive width and height.");
  }

  return bounds;
}

/**
 * Read all four scale and translation values from `spec/display-optimization.md#multiscale-transform`.
 * Reject Z and M data before this step.
 */
function parseTransform(value: unknown): QuantizationTransform {
  if (!isRecord(value)) {
    throw new Error("Each geodisplay LOD transform must contain an object.");
  }

  return {
    scale: parseNumberTuple(value.scale, "transform.scale"),
    translate: parseNumberTuple(value.translate, "transform.translate"),
  };
}

function parseNumberTuple(
  value: unknown,
  name: string,
): [number, number, number, number] {
  if (
    !Array.isArray(value) ||
    value.length !== 4 ||
    !value.every((entry) => typeof entry === "number" && Number.isFinite(entry))
  ) {
    throw new Error(`${name} must contain four finite numbers.`);
  }

  return [value[0], value[1], value[2], value[3]];
}

/**
 * Verify that the metadata path names a physical leaf column in the first row group.
 */
function validatePhysicalColumn(
  metadata: FileMetaData,
  path: readonly string[],
): void {
  const fieldName = path.join(".");
  const exists = metadata.row_groups[0]?.columns.some(
    ({ meta_data: column }) => column?.path_in_schema.join(".") === fieldName,
  );

  if (!exists) {
    throw new Error(`Parquet column ${fieldName} was not found.`);
  }
}

function requireInteger(value: unknown, name: string): number {
  const number = requireNumber(value, name);
  if (!Number.isInteger(number) || number < 0) {
    throw new Error(`${name} must contain a non-negative integer.`);
  }

  return number;
}

function requirePositiveNumber(value: unknown, name: string): number {
  const number = requireNumber(value, name);
  if (number <= 0) {
    throw new Error(`${name} must contain a positive number.`);
  }

  return number;
}

function requireNumber(value: unknown, name: string): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw new Error(`${name} must contain a finite number.`);
  }

  return value;
}

/**
 * The SOP root path uses text or a two-part tuple.
 * The MapLibre reader also accepts a nonempty text array.
 * The reader resolves each accepted path against physical Parquet columns.
 * The reader does not use Spark field metadata.
 */
function normalizeColumnPath(value: unknown, name: string): string[] {
  if (typeof value === "string") {
    if (value.length === 0) {
      throw new TypeError(`${name} must contain a non-empty column path.`);
    }
    return [value];
  }

  if (
    !Array.isArray(value) ||
    value.length === 0 ||
    !value.every(
      (part) => typeof part === "string" && part.length > 0,
    )
  ) {
    throw new TypeError(
      `${name} must contain a non-empty string or an array of non-empty strings.`,
    );
  }

  return [...value];
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

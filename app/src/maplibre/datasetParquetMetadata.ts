import {
  parquetMetadataAsync,
  type AsyncBuffer,
  type FileMetaData,
} from "hyparquet";

import { formatRatio } from "../common/formatNumber";

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
  columnPath: [string, string];
  level: number;
  resolution: number;
  transform: QuantizationTransform;
}

export interface XZDisplayMetadata {
  codePath: [string];
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

  return {   compression: formatCompression(metadata),
  file: metadata, display };
}

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

function parseXZDisplayMetadata(value: unknown): XZDisplayMetadata {
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

  const code = requireString(value.code, "geodisplay.code");
  const maxLevel = requireInteger(value.maxLevel, "geodisplay.maxLevel");
  const fullExtent = parseBounds(value.fullExtent);
  if (!Array.isArray(value.levels) || value.levels.length === 0) {
    throw new Error("Geodisplay metadata must define at least one LOD level.");
  }

  const levels = value.levels
    .map(parseLODLevel)
    .sort((left, right) => left.level - right.level);

  return {
    codePath: [code],
    fullExtent,
    geometryType: value.geometryType,
    maxLevel,
    levels,
    version: typeof value.version === "string" ? value.version : null,
  };
}

function parseLODLevel(value: unknown): LODLevel {
  if (!isRecord(value)) {
    throw new Error("Each geodisplay LOD level must contain an object.");
  }
  if (
    !Array.isArray(value.column) ||
    value.column.length !== 2 ||
    !value.column.every((part) => typeof part === "string")
  ) {
    throw new Error("Each geodisplay LOD column must contain a two-part path.");
  }

  return {
    columnPath: [value.column[0], value.column[1]],
    level: requireInteger(value.level, "geodisplay.levels[].level"),
    resolution: requirePositiveNumber(
      value.resolution,
      "geodisplay.levels[].resolution",
    ),
    transform: parseTransform(value.transform),
  };
}

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

function requireString(value: unknown, name: string): string {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${name} must contain a non-empty string.`);
  }

  return value;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

export interface ArcgisEventHandle {
  remove(): void;
}

export interface ArcgisParquetDiagnosticsSource {
  on(
    eventName: "range-read",
    listener: (event: unknown) => void,
  ): ArcgisEventHandle;
  getDiagnosticsSnapshot(): Promise<unknown>;
}

export interface ArcgisParquetByteRange {
  start: number;
  end: number;
}

export type ArcgisParquetNumericValue =
  | { type: "int32"; value: number }
  | { type: "int64"; value: string }
  | { type: "float32"; value: number }
  | { type: "float64"; value: number };

export interface ArcgisParquetColumnDiagnosticsV1 {
  index: number;
  path: string[];
  name: string;
  physicalType: string;
  logicalType: string | null;
  nullable: boolean;
  maxDefinitionLevel: number;
  maxRepetitionLevel: number;
}

export interface ArcgisParquetColumnChunkStatisticsV1 {
  numValues: number;
  nullCount: number | null;
  distinctCount: number | null;
  nanCount: number | null;
  min: ArcgisParquetNumericValue | null;
  max: ArcgisParquetNumericValue | null;
  minExact: boolean | null;
  maxExact: boolean | null;
}

export interface ArcgisParquetColumnChunkDiagnosticsV1 {
  columnIndex: number;
  dataRange: ArcgisParquetByteRange;
  columnIndexRange: ArcgisParquetByteRange | null;
  offsetIndexRange: ArcgisParquetByteRange | null;
  statistics: ArcgisParquetColumnChunkStatisticsV1;
  compression: string;
  encodings: string[];
  compressedSize: number;
  uncompressedSize: number;
  dictionaryPageOffset: number | null;
  dataPageOffset: number;
}

export interface ArcgisParquetGeospatialBoundsV1 {
  xmin: number;
  xmax: number;
  ymin: number;
  ymax: number;
  zmin: number | null;
  zmax: number | null;
  mmin: number | null;
  mmax: number | null;
}

export interface ArcgisParquetRowGroupDiagnosticsV1 {
  index: number;
  rowStart: number;
  rowCount: number;
  dataRange: ArcgisParquetByteRange | null;
  bounds: ArcgisParquetGeospatialBoundsV1 | null;
  columns: ArcgisParquetColumnChunkDiagnosticsV1[];
}

export interface ArcgisParquetFileDiagnosticsV1 {
  version: 1;
  fileId: number;
  fileName: string;
  byteLength: number;
  footerRange: ArcgisParquetByteRange;
  keyValueMetadata: ArcgisParquetKeyValueMetadataV1[];
  columns: ArcgisParquetColumnDiagnosticsV1[];
  rowGroups: ArcgisParquetRowGroupDiagnosticsV1[];
}

export interface ArcgisParquetKeyValueMetadataV1 {
  key: string;
  value: string | null;
}

export interface ArcgisParquetDiagnosticsSnapshotV1 {
  files: ArcgisParquetFileDiagnosticsV1[];
}

interface ArcgisParquetRangeReadEventBase {
  fileId: string;
  requestId: number;
  range: ArcgisParquetByteRange;
}

export interface ArcgisParquetRangeReadStartEvent
  extends ArcgisParquetRangeReadEventBase {
  phase: "start";
}

export interface ArcgisParquetRangeReadCompleteEvent
  extends ArcgisParquetRangeReadEventBase {
  phase: "complete";
}

export interface ArcgisParquetRangeReadErrorEvent
  extends ArcgisParquetRangeReadEventBase {
  phase: "error";
}

export type ArcgisParquetRangeReadEvent =
  | ArcgisParquetRangeReadStartEvent
  | ArcgisParquetRangeReadCompleteEvent
  | ArcgisParquetRangeReadErrorEvent;

export function resolveParquetDiagnosticsSource(
  layer: unknown,
): ArcgisParquetDiagnosticsSource {
  const layerRecord = readRecord(layer, "Parquet layer");
  const source = layerRecord.source;
  if (!isParquetDiagnosticsSource(source)) {
    throw new TypeError(
      "Parquet layer source does not expose the diagnostics runtime API.",
    );
  }

  return source;
}

export function parseParquetDiagnosticsSnapshot(
  value: unknown,
): ArcgisParquetDiagnosticsSnapshotV1 {
  const snapshot = readRecord(value, "diagnostics snapshot");
  const files = readArray(snapshot.files, "diagnostics snapshot.files").map(
    (file, fileIndex) => parseFileDiagnostics(file, `diagnostics snapshot.files[${fileIndex}]`),
  );
  const fileIds = new Set<number>();
  for (const file of files) {
    if (fileIds.has(file.fileId)) {
      throw new TypeError(`Duplicate diagnostics fileId "${file.fileId}".`);
    }
    fileIds.add(file.fileId);
  }

  return { files };
}

export function parseParquetRangeReadEvent(
  value: unknown,
): ArcgisParquetRangeReadEvent {
  const event = readRecord(value, "range-read event");
  const fileId = readNonEmptyString(event.fileId, "range-read event.fileId");
  const requestId = readStructuralInteger(event.requestId, "range-read event.requestId");
  const range = readByteRange(event.range, "range-read event.range", false);
  const eventBase = { fileId, requestId, range };
  switch (event.phase) {
    case "start":
      return {
        ...eventBase,
        phase: "start",
      };
    case "complete": {
      return {
        ...eventBase,
        phase: "complete",
      };
    }
    case "error":
      return {
        ...eventBase,
        phase: "error",
      };
    default:
      throw new TypeError(`Invalid range-read event.phase "${String(event.phase)}".`);
  }
}

function parseFileDiagnostics(
  value: unknown,
  name: string,
): ArcgisParquetFileDiagnosticsV1 {
  const file = readRecord(value, name);
  readVersion(file.version, `${name}.version`);
  const fileId = readStructuralInteger(file.fileId, `${name}.fileId`);
  const fileName = readNonEmptyString(file.fileName, `${name}.fileName`);
  const byteLength = readStructuralInteger(file.byteLength, `${name}.byteLength`);
  const footerRange = readBoundedByteRange(
    file.footerRange,
    `${name}.footerRange`,
    byteLength,
  );
  const keyValueMetadata = readArray(
    file.keyValueMetadata,
    `${name}.keyValueMetadata`,
  ).map((item, itemIndex) => {
    const entryName = `${name}.keyValueMetadata[${itemIndex}]`;
    const entry = readRecord(item, entryName);
    return {
      key: readNonEmptyString(entry.key, `${entryName}.key`),
      value: readNullableString(entry.value, `${entryName}.value`),
    };
  });
  const columns = readArray(file.columns, `${name}.columns`).map(
    (column, columnIndex) => parseColumnDiagnostics(column, `${name}.columns[${columnIndex}]`),
  );
  const rowGroups = readArray(file.rowGroups, `${name}.rowGroups`).map(
    (rowGroup, rowGroupIndex) =>
      parseRowGroupDiagnostics(
        rowGroup,
        `${name}.rowGroups[${rowGroupIndex}]`,
        byteLength,
        columns.length,
      ),
  );

  validateIndexedSequence(columns, `${name}.columns`);
  validateIndexedSequence(rowGroups, `${name}.rowGroups`);
  return {
    version: 1,
    fileId,
    fileName,
    byteLength,
    footerRange,
    keyValueMetadata,
    columns,
    rowGroups,
  };
}

function parseColumnDiagnostics(
  value: unknown,
  name: string,
): ArcgisParquetColumnDiagnosticsV1 {
  const column = readRecord(value, name);
  return {
    index: readStructuralInteger(column.index, `${name}.index`),
    path: readArray(column.path, `${name}.path`).map((part, pathIndex) =>
      readNonEmptyString(part, `${name}.path[${pathIndex}]`),
    ),
    name: readNonEmptyString(column.name, `${name}.name`),
    physicalType: readNonEmptyString(column.physicalType, `${name}.physicalType`),
    logicalType: readNullableString(column.logicalType, `${name}.logicalType`),
    nullable: readBoolean(column.nullable, `${name}.nullable`),
    maxDefinitionLevel: readStructuralInteger(
      column.maxDefinitionLevel,
      `${name}.maxDefinitionLevel`,
    ),
    maxRepetitionLevel: readStructuralInteger(
      column.maxRepetitionLevel,
      `${name}.maxRepetitionLevel`,
    ),
  };
}

function parseRowGroupDiagnostics(
  value: unknown,
  name: string,
  byteLength: number,
  columnCount: number,
): ArcgisParquetRowGroupDiagnosticsV1 {
  const rowGroup = readRecord(value, name);
  const rowStart = readStructuralInteger(rowGroup.rowStart, `${name}.rowStart`);
  const rowCount = readStructuralInteger(rowGroup.rowCount, `${name}.rowCount`);
  if (!Number.isSafeInteger(rowStart + rowCount)) {
    throw new TypeError(`${name} row range exceeds the safe integer range.`);
  }

  const columns = readArray(rowGroup.columns, `${name}.columns`).map(
    (column, columnIndex) =>
      parseColumnChunkDiagnostics(
        column,
        `${name}.columns[${columnIndex}]`,
        byteLength,
        columnCount,
      ),
  );
  const columnIndexes = new Set<number>();
  for (const column of columns) {
    if (columnIndexes.has(column.columnIndex)) {
      throw new TypeError(`${name} contains duplicate columnIndex ${column.columnIndex}.`);
    }
    columnIndexes.add(column.columnIndex);
  }

  return {
    index: readStructuralInteger(rowGroup.index, `${name}.index`),
    rowStart,
    rowCount,
    dataRange: readNullableBoundedByteRange(
      rowGroup.dataRange,
      `${name}.dataRange`,
      byteLength,
    ),
    bounds: readNullableBounds(rowGroup.bounds, `${name}.bounds`),
    columns,
  };
}

function parseColumnChunkDiagnostics(
  value: unknown,
  name: string,
  byteLength: number,
  columnCount: number,
): ArcgisParquetColumnChunkDiagnosticsV1 {
  const chunk = readRecord(value, name);
  const columnIndex = readStructuralInteger(chunk.columnIndex, `${name}.columnIndex`);
  if (columnIndex >= columnCount) {
    throw new TypeError(`${name}.columnIndex does not identify a file column.`);
  }

  return {
    columnIndex,
    dataRange: readBoundedByteRange(chunk.dataRange, `${name}.dataRange`, byteLength),
    columnIndexRange: readNullableBoundedByteRange(
      chunk.columnIndexRange,
      `${name}.columnIndexRange`,
      byteLength,
    ),
    offsetIndexRange: readNullableBoundedByteRange(
      chunk.offsetIndexRange,
      `${name}.offsetIndexRange`,
      byteLength,
    ),
    statistics: parseChunkStatistics(chunk.statistics, `${name}.statistics`),
    compression: readNonEmptyString(chunk.compression, `${name}.compression`),
    encodings: readArray(chunk.encodings, `${name}.encodings`).map(
      (encoding, index) =>
        readNonEmptyString(encoding, `${name}.encodings[${index}]`),
    ),
    compressedSize: readStructuralInteger(
      chunk.compressedSize,
      `${name}.compressedSize`,
    ),
    uncompressedSize: readStructuralInteger(
      chunk.uncompressedSize,
      `${name}.uncompressedSize`,
    ),
    dictionaryPageOffset:
      chunk.dictionaryPageOffset === null
        ? null
        : readStructuralInteger(
            chunk.dictionaryPageOffset,
            `${name}.dictionaryPageOffset`,
          ),
    dataPageOffset: readStructuralInteger(
      chunk.dataPageOffset,
      `${name}.dataPageOffset`,
    ),
  };
}

function parseChunkStatistics(
  value: unknown,
  name: string,
): ArcgisParquetColumnChunkStatisticsV1 {
  const statistics = readRecord(value, name);
  return {
    numValues: readStructuralInteger(statistics.numValues, `${name}.numValues`),
    nullCount: readNullableStructuralInteger(statistics.nullCount, `${name}.nullCount`),
    distinctCount: readNullableStructuralInteger(
      statistics.distinctCount,
      `${name}.distinctCount`,
    ),
    nanCount: readNullableStructuralInteger(statistics.nanCount, `${name}.nanCount`),
    min: readNullableNumericValue(statistics.min, `${name}.min`),
    max: readNullableNumericValue(statistics.max, `${name}.max`),
    minExact: readNullableBoolean(statistics.minExact, `${name}.minExact`),
    maxExact: readNullableBoolean(statistics.maxExact, `${name}.maxExact`),
  };
}

function readNullableNumericValue(
  value: unknown,
  name: string,
): ArcgisParquetNumericValue | null {
  if (value === null) {
    return null;
  }
  const numeric = readRecord(value, name);
  switch (numeric.type) {
    case "int32": {
      const integer = readFiniteNumber(numeric.value, `${name}.value`);
      if (!Number.isInteger(integer) || integer < -2_147_483_648 || integer > 2_147_483_647) {
        throw new TypeError(`Invalid ${name}.value.`);
      }
      return { type: "int32", value: integer };
    }
    case "int64": {
      const integer = readString(numeric.value, `${name}.value`);
      if (!/^-?(?:0|[1-9]\d*)$/.test(integer)) {
        throw new TypeError(`Invalid ${name}.value.`);
      }
      return { type: "int64", value: integer };
    }
    case "float32":
    case "float64":
      return {
        type: numeric.type,
        value: readFiniteNumber(numeric.value, `${name}.value`),
      };
    default:
      throw new TypeError(`Invalid ${name}.type.`);
  }
}

function readNullableBounds(
  value: unknown,
  name: string,
): ArcgisParquetGeospatialBoundsV1 | null {
  if (value === null) {
    return null;
  }
  const bounds = readRecord(value, name);
  const xmin = readFiniteNumber(bounds.xmin, `${name}.xmin`);
  const xmax = readFiniteNumber(bounds.xmax, `${name}.xmax`);
  const ymin = readFiniteNumber(bounds.ymin, `${name}.ymin`);
  const ymax = readFiniteNumber(bounds.ymax, `${name}.ymax`);
  if (xmax < xmin || ymax < ymin) {
    throw new TypeError(`${name} maximum values must not precede minimum values.`);
  }
  const zmin = readNullableFiniteNumber(bounds.zmin, `${name}.zmin`);
  const zmax = readNullableFiniteNumber(bounds.zmax, `${name}.zmax`);
  const mmin = readNullableFiniteNumber(bounds.mmin, `${name}.mmin`);
  const mmax = readNullableFiniteNumber(bounds.mmax, `${name}.mmax`);
  validateOptionalBounds(zmin, zmax, `${name}.z`);
  validateOptionalBounds(mmin, mmax, `${name}.m`);

  return {
    xmin,
    xmax,
    ymin,
    ymax,
    zmin,
    zmax,
    mmin,
    mmax,
  };
}

function validateOptionalBounds(
  minimum: number | null,
  maximum: number | null,
  name: string,
): void {
  if ((minimum === null) !== (maximum === null)) {
    throw new TypeError(`${name} bounds must provide both minimum and maximum values.`);
  }
  if (minimum !== null && maximum !== null && maximum < minimum) {
    throw new TypeError(`${name} maximum must not precede its minimum.`);
  }
}

function readBoundedByteRange(
  value: unknown,
  name: string,
  byteLength: number,
): ArcgisParquetByteRange {
  const range = readByteRange(value, name, false);
  if (range.end > byteLength) {
    throw new TypeError(`${name} exceeds the file byteLength.`);
  }
  return range;
}

function readNullableBoundedByteRange(
  value: unknown,
  name: string,
  byteLength: number,
): ArcgisParquetByteRange | null {
  return value === null ? null : readBoundedByteRange(value, name, byteLength);
}

function readByteRange(
  value: unknown,
  name: string,
  allowEmpty: boolean,
): ArcgisParquetByteRange {
  const range = readRecord(value, name);
  const start = readStructuralInteger(range.start, `${name}.start`);
  const end = readStructuralInteger(range.end, `${name}.end`);
  if (end < start || (!allowEmpty && end === start)) {
    throw new TypeError(`${name} must be a non-empty half-open byte range.`);
  }
  return { start, end };
}

function validateIndexedSequence(
  values: ReadonlyArray<{ index: number }>,
  name: string,
): void {
  values.forEach((value, index) => {
    if (value.index !== index) {
      throw new TypeError(`${name}[${index}].index must equal ${index}.`);
    }
  });
}

function isParquetDiagnosticsSource(
  value: unknown,
): value is ArcgisParquetDiagnosticsSource {
  return (
    isRecord(value) &&
    typeof value.on === "function" &&
    typeof value.getDiagnosticsSnapshot === "function"
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function readRecord(value: unknown, name: string): Record<string, unknown> {
  if (!isRecord(value)) {
    throw new TypeError(`Invalid ${name}.`);
  }
  return value;
}

function readArray(value: unknown, name: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new TypeError(`Invalid ${name}.`);
  }
  return value;
}

function readVersion(value: unknown, name: string): 1 {
  if (value !== 1) {
    throw new TypeError(`Unsupported ${name} ${String(value)}.`);
  }
  return 1;
}

function readStructuralInteger(value: unknown, name: string): number {
  if (!Number.isSafeInteger(value) || typeof value !== "number" || value < 0) {
    throw new TypeError(`Invalid ${name}.`);
  }
  return value;
}

function readNullableStructuralInteger(value: unknown, name: string): number | null {
  return value === null ? null : readStructuralInteger(value, name);
}

function readFiniteNumber(value: unknown, name: string): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw new TypeError(`Invalid ${name}.`);
  }
  return value;
}

function readNullableFiniteNumber(value: unknown, name: string): number | null {
  return value === null ? null : readFiniteNumber(value, name);
}

function readString(value: unknown, name: string): string {
  if (typeof value !== "string") {
    throw new TypeError(`Invalid ${name}.`);
  }
  return value;
}

function readNonEmptyString(value: unknown, name: string): string {
  const string = readString(value, name);
  if (string.length === 0) {
    throw new TypeError(`Invalid ${name}.`);
  }
  return string;
}

function readNullableString(value: unknown, name: string): string | null {
  return value === null ? null : readString(value, name);
}

function readBoolean(value: unknown, name: string): boolean {
  if (typeof value !== "boolean") {
    throw new TypeError(`Invalid ${name}.`);
  }
  return value;
}

function readNullableBoolean(value: unknown, name: string): boolean | null {
  return value === null ? null : readBoolean(value, name);
}

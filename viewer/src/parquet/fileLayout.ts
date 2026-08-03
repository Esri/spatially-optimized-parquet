export interface ParquetByteRange {
  start: number;
  end: number;
}

export type ParquetNumericValue =
  | { type: "int32"; value: number }
  | { type: "int64"; value: string }
  | { type: "float32"; value: number }
  | { type: "float64"; value: number };

export interface ParquetColumnDiagnostics {
  index: number;
  path: string[];
  name: string;
  physicalType: string;
  logicalType: string | null;
  nullable: boolean;
  maxDefinitionLevel: number;
  maxRepetitionLevel: number;
}

export interface ParquetColumnChunkStatistics {
  numValues: number;
  nullCount: number | null;
  distinctCount: number | null;
  nanCount: number | null;
  min: ParquetNumericValue | null;
  max: ParquetNumericValue | null;
  minExact: boolean | null;
  maxExact: boolean | null;
}

export interface ParquetColumnChunkDiagnostics {
  columnIndex: number;
  dataRange: ParquetByteRange;
  columnIndexRange: ParquetByteRange | null;
  offsetIndexRange: ParquetByteRange | null;
  statistics: ParquetColumnChunkStatistics;
  compression: string;
  encodings: string[];
  compressedSize: number;
  uncompressedSize: number;
  dictionaryPageOffset: number | null;
  dataPageOffset: number;
}

export interface ParquetGeospatialBounds {
  xmin: number;
  xmax: number;
  ymin: number;
  ymax: number;
  zmin: number | null;
  zmax: number | null;
  mmin: number | null;
  mmax: number | null;
}

export interface ParquetRowGroupDiagnostics {
  index: number;
  rowStart: number;
  rowCount: number;
  dataRange: ParquetByteRange | null;
  bounds: ParquetGeospatialBounds | null;
  columns: ParquetColumnChunkDiagnostics[];
}

export interface ParquetKeyValueMetadata {
  key: string;
  value: string | null;
}

export interface ParquetFileDiagnostics {
  version: 1;
  fileId: number;
  fileName: string;
  byteLength: number;
  footerRange: ParquetByteRange;
  keyValueMetadata: ParquetKeyValueMetadata[];
  columns: ParquetColumnDiagnostics[];
  rowGroups: ParquetRowGroupDiagnostics[];
}

export interface ParquetDiagnosticsSnapshot {
  files: ParquetFileDiagnostics[];
}

export type ByteRange = ParquetByteRange;
export type ColumnStatisticValue = number | string;

export interface ColumnLayout {
  id: string;
  rowGroupIndex: number;
  columnIndex: number;
  fieldName: string;
  byteRange: ByteRange;
  minimumValue: ColumnStatisticValue | null;
  maximumValue: ColumnStatisticValue | null;
  nullCount: number | null;
  recordCount: number;
  valueCount: number;
  physicalType: string;
  logicalType: string | null;
  nullable: boolean;
  maxDefinitionLevel: number;
  maxRepetitionLevel: number;
  compression: string;
  encodings: string[];
  compressedSize: number;
  uncompressedSize: number;
  dictionaryPageOffset: number | null;
  dataPageOffset: number;
}

export interface RowGroupLayout {
  index: number;
  byteRange: ByteRange;
  columns: ColumnLayout[];
}

export interface PageIndexLayout {
  id: string;
  rowGroupIndex: number;
  fieldName: string;
  kind: "column" | "offset";
  byteRange: ByteRange;
  minimumValue: ColumnStatisticValue | null;
  maximumValue: ColumnStatisticValue | null;
  nullCount: number | null;
  recordCount: number;
}

export interface FileLayout {
  fileId: number;
  fileName: string;
  byteLength: number;
  footer: ByteRange;
  keyValueMetadata: ParquetKeyValueMetadata[];
  rowGroups: RowGroupLayout[];
  pageIndexes: PageIndexLayout[];
}

export function deriveFileLayout(
  snapshot: ParquetDiagnosticsSnapshot,
  file: ParquetFileDiagnostics,
): FileLayout {
  assertSnapshotFile(snapshot, file);
  const pageIndexes: PageIndexLayout[] = [];
  const rowGroups = file.rowGroups.map((rowGroup) => {
    const columns = rowGroup.columns.map((chunk) => {
      const column = file.columns[chunk.columnIndex];
      const fieldName = column.path.join(".") || column.name;
      pageIndexes.push(
        ...derivePageIndexes(chunk, {
          fieldName,
          rowGroupIndex: rowGroup.index,
          columnIndex: chunk.columnIndex,
          recordCount: rowGroup.rowCount,
        }),
      );

      return {
        id: `rg${rowGroup.index}-c${chunk.columnIndex}`,
        rowGroupIndex: rowGroup.index,
        columnIndex: chunk.columnIndex,
        fieldName,
        byteRange: chunk.dataRange,
        minimumValue: chunk.statistics.min?.value ?? null,
        maximumValue: chunk.statistics.max?.value ?? null,
        nullCount: chunk.statistics.nullCount,
        recordCount: rowGroup.rowCount,
        valueCount: chunk.statistics.numValues,
        physicalType: column.physicalType,
        logicalType: column.logicalType,
        nullable: column.nullable,
        maxDefinitionLevel: column.maxDefinitionLevel,
        maxRepetitionLevel: column.maxRepetitionLevel,
        compression: chunk.compression,
        encodings: chunk.encodings,
        compressedSize: chunk.compressedSize,
        uncompressedSize: chunk.uncompressedSize,
        dictionaryPageOffset: chunk.dictionaryPageOffset,
        dataPageOffset: chunk.dataPageOffset,
      };
    });
    const byteRange = rowGroup.dataRange ?? deriveEnvelope(
      columns.map((column) => column.byteRange),
    );
    if (!byteRange) {
      throw new Error(`Parquet row group ${rowGroup.index} has no data byte range.`);
    }

    return {
      index: rowGroup.index,
      byteRange,
      columns,
    };
  });

  return {
    fileId: file.fileId,
    fileName: file.fileName,
    byteLength: file.byteLength,
    footer: file.footerRange,
    keyValueMetadata: file.keyValueMetadata,
    rowGroups,
    pageIndexes,
  };
}

export function rangesOverlap(first: ByteRange, second: ByteRange): boolean {
  return first.start < second.end && second.start < first.end;
}

function assertSnapshotFile(
  snapshot: ParquetDiagnosticsSnapshot,
  file: ParquetFileDiagnostics,
): void {
  const snapshotFile = snapshot.files.find(
    (candidate) =>
      candidate.fileId === file.fileId && candidate.fileName === file.fileName,
  );
  if (snapshotFile !== file) {
    throw new Error("The selected diagnostics file does not belong to the snapshot.");
  }
}

function derivePageIndexes(
  chunk: ParquetFileDiagnostics["rowGroups"][number]["columns"][number],
  {
    fieldName,
    rowGroupIndex,
    columnIndex,
    recordCount,
  }: {
    fieldName: string;
    rowGroupIndex: number;
    columnIndex: number;
    recordCount: number;
  },
): PageIndexLayout[] {
  return [
    createPageIndexLayout("column", chunk.columnIndexRange),
    createPageIndexLayout("offset", chunk.offsetIndexRange),
  ].flatMap((pageIndex) => {
    if (!pageIndex) {
      return [];
    }

    return [{
      ...pageIndex,
      id: `rg${rowGroupIndex}-c${columnIndex}-${pageIndex.kind}-index`,
      fieldName,
      rowGroupIndex,
      minimumValue: chunk.statistics.min?.value ?? null,
      maximumValue: chunk.statistics.max?.value ?? null,
      nullCount: chunk.statistics.nullCount,
      recordCount,
    }];
  });
}

function createPageIndexLayout(
  kind: PageIndexLayout["kind"],
  byteRange: ByteRange | null,
): Pick<PageIndexLayout, "kind" | "byteRange"> | null {
  return byteRange ? { kind, byteRange } : null;
}

function deriveEnvelope(ranges: ByteRange[]): ByteRange | null {
  if (ranges.length === 0) {
    return null;
  }

  return {
    start: Math.min(...ranges.map((range) => range.start)),
    end: Math.max(...ranges.map((range) => range.end)),
  };
}

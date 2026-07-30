import type {
  ArcgisParquetByteRange,
  ArcgisParquetDiagnosticsSnapshotV1,
  ArcgisParquetFileDiagnosticsV1,
  ArcgisParquetKeyValueMetadataV1,
} from "./arcgisParquetDiagnostics";

export type ByteRange = ArcgisParquetByteRange;
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
  keyValueMetadata: ArcgisParquetKeyValueMetadataV1[];
  rowGroups: RowGroupLayout[];
  pageIndexes: PageIndexLayout[];
}

export function deriveFileLayout(
  snapshot: ArcgisParquetDiagnosticsSnapshotV1,
  file: ArcgisParquetFileDiagnosticsV1 = resolveSingleDiagnosticsFile(snapshot),
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

export function resolveSingleDiagnosticsFile(
  snapshot: ArcgisParquetDiagnosticsSnapshotV1,
): ArcgisParquetFileDiagnosticsV1 {
  if (snapshot.files.length !== 1) {
    throw new Error(
      `The Parquet download UI currently requires exactly one diagnostics file, received ${snapshot.files.length}.`,
    );
  }
  return snapshot.files[0];
}

export function rangesOverlap(first: ByteRange, second: ByteRange): boolean {
  return first.start < second.end && second.start < first.end;
}

function assertSnapshotFile(
  snapshot: ArcgisParquetDiagnosticsSnapshotV1,
  file: ArcgisParquetFileDiagnosticsV1,
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
  chunk: ArcgisParquetFileDiagnosticsV1["rowGroups"][number]["columns"][number],
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

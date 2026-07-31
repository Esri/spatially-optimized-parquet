import type { FileLayout } from "./fileLayout";

export interface FileDetailSummary {
  rowCount: number;
  columnCount: number;
  rowGroupCount: number;
  compressionCodecs: readonly string[];
  compressedSize: number;
  uncompressedSize: number;
}

export interface DatasetDetailSummary extends FileDetailSummary {
  byteLength: number;
  fileCount: number;
}

export function deriveFileDetailSummary(
  layout: FileLayout,
): FileDetailSummary {
  const compressionCodecs = new Set<string>();
  const columnNames = new Set<string>();
  let compressedSize = 0;
  let uncompressedSize = 0;

  for (const rowGroup of layout.rowGroups) {
    for (const column of rowGroup.columns) {
      compressionCodecs.add(column.compression);
      columnNames.add(column.fieldName);
      compressedSize += column.compressedSize;
      uncompressedSize += column.uncompressedSize;
    }
  }

  return {
    rowCount: layout.rowGroups.reduce(
      (total, rowGroup) => total + (rowGroup.columns[0]?.recordCount ?? 0),
      0,
    ),
    columnCount: columnNames.size,
    rowGroupCount: layout.rowGroups.length,
    compressionCodecs: [...compressionCodecs].sort(),
    compressedSize,
    uncompressedSize,
  };
}

export function deriveDatasetDetailSummary(
  layouts: readonly FileLayout[],
): DatasetDetailSummary {
  const compressionCodecs = new Set<string>();
  const columnNames = new Set<string>();
  let byteLength = 0;
  let rowCount = 0;
  let rowGroupCount = 0;
  let compressedSize = 0;
  let uncompressedSize = 0;

  for (const layout of layouts) {
    const summary = deriveFileDetailSummary(layout);
    byteLength += layout.byteLength;
    rowCount += summary.rowCount;
    rowGroupCount += summary.rowGroupCount;
    compressedSize += summary.compressedSize;
    uncompressedSize += summary.uncompressedSize;
    for (const codec of summary.compressionCodecs) {
      compressionCodecs.add(codec.toUpperCase());
    }
    for (const rowGroup of layout.rowGroups) {
      for (const column of rowGroup.columns) {
        columnNames.add(column.fieldName);
      }
    }
  }

  return {
    byteLength,
    fileCount: layouts.length,
    rowCount,
    columnCount: columnNames.size,
    rowGroupCount,
    compressionCodecs: [...compressionCodecs].sort(),
    compressedSize,
    uncompressedSize,
  };
}

import type { FileLayout } from "./parquetFileLayout";

export interface FileDetailSummary {
  rowCount: number;
  columnCount: number;
  rowGroupCount: number;
  compressionCodecs: readonly string[];
  compressedSize: number;
  uncompressedSize: number;
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

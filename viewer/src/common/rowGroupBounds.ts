import type {
  ParquetDiagnosticsSnapshot,
  ParquetFileDiagnostics,
} from "../parquet/fileLayout";

export interface RowGroupBound {
  fileId: number;
  fileName: string;
  rowGroupIndex: number;
  featureCount: number;
  xmin: number;
  ymin: number;
  xmax: number;
  ymax: number;
}

export interface BoundsExtent {
  xmin: number;
  ymin: number;
  xmax: number;
  ymax: number;
}

export function deriveRowGroupBounds(
  snapshot: ParquetDiagnosticsSnapshot,
  file: ParquetFileDiagnostics,
): RowGroupBound[] {
  if (!snapshot.files.includes(file)) {
    throw new Error("The selected diagnostics file does not belong to the snapshot.");
  }

  return file.rowGroups.flatMap((rowGroup) =>
    rowGroup.bounds
      ? [{
          fileId: file.fileId,
          fileName: file.fileName,
          rowGroupIndex: rowGroup.index,
          featureCount: rowGroup.rowCount,
          xmin: rowGroup.bounds.xmin,
          ymin: rowGroup.bounds.ymin,
          xmax: rowGroup.bounds.xmax,
          ymax: rowGroup.bounds.ymax,
        }]
      : []
  );
}

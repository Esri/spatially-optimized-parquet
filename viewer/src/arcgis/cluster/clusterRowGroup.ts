import type { ParquetFileDiagnostics } from "../../parquet/fileLayout";
import {
  getParquetFileId,
  getParquetObjectId,
  getParquetRowId,
} from "../parquetObjectId";

export interface ClusterRowGroupSelection {
  objectIdEnd: number;
  objectIdStart: number;
}

export function resolveClusterRowGroup(
  files: readonly ParquetFileDiagnostics[],
  objectId: number,
): ClusterRowGroupSelection | null {
  const fileId = getParquetFileId(objectId);
  const rowId = getParquetRowId(objectId);
  const file = files.find((candidate) => candidate.fileId === fileId);
  if (!file) {
    return null;
  }

  let low = 0;
  let high = file.rowGroups.length - 1;
  while (low <= high) {
    const middle = Math.floor((low + high) / 2);
    const rowGroup = file.rowGroups[middle];
    if (rowId < rowGroup.rowStart) {
      high = middle - 1;
    } else if (rowId >= rowGroup.rowStart + rowGroup.rowCount) {
      low = middle + 1;
    } else {
      return {
        objectIdStart: getParquetObjectId(fileId, rowGroup.rowStart),
        objectIdEnd: getParquetObjectId(
          fileId,
          rowGroup.rowStart + rowGroup.rowCount,
        ),
      };
    }
  }
  return null;
}

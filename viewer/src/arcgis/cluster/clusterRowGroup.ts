// Copyright 2026 Esri
//
// Licensed under the Apache License Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

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

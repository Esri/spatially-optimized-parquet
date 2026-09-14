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

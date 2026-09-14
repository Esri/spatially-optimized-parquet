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

import type { ParquetNumericValue } from "../../diagnostics";

export interface ParquetPageIndexTarget {
  fileId: number;
  rowGroupIndex: number;
  columnIndex: number;
}

export type ParquetPageStatistic =
  | { type: "nullOnly" }
  | {
      type: "bounds";
      min: ParquetNumericValue;
      max: ParquetNumericValue;
    }
  | { type: "unknown" };

export interface ParquetColumnIndex {
  pages: ParquetPageStatistic[];
}

export interface ParquetPageLocation {
  pageIndex: number;
  rowStart: number;
  rowEnd: number;
  byteStart: number;
  byteEnd: number;
  compressedPageSize: number;
}

export interface ParquetOffsetIndex {
  pages: ParquetPageLocation[];
}

export interface ParquetPageIndexSource {
  getColumnIndex(
    target: ParquetPageIndexTarget,
  ): Promise<ParquetColumnIndex | null>;
  getOffsetIndex(
    target: ParquetPageIndexTarget,
  ): Promise<ParquetOffsetIndex | null>;
}

export function resolveParquetPageIndexSource(
  value: unknown,
): ParquetPageIndexSource {
  if (
    typeof value !== "object" ||
    value === null ||
    !("getColumnIndex" in value) ||
    typeof value.getColumnIndex !== "function" ||
    !("getOffsetIndex" in value) ||
    typeof value.getOffsetIndex !== "function"
  ) {
    throw new TypeError("Parquet source does not expose page-index methods.");
  }
  return value as ParquetPageIndexSource;
}

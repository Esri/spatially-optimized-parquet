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

import type { ParquetByteCoverage } from "../../../parquet/ParquetByteCoverage";
import type { ByteRange } from "../../../parquet/fileLayout";

export function orderByLoadedByteLength<T extends { byteRange: ByteRange }>(
  items: readonly T[],
  coverage: ParquetByteCoverage,
  enabled: boolean,
): T[] {
  return orderByDescendingValue(
    items,
    (item) => coverage.coveredByteLength(item.byteRange),
    enabled,
  );
}

export function orderByDescendingValue<T>(
  items: readonly T[],
  value: (item: T) => number,
  enabled: boolean,
): T[] {
  if (!enabled) {
    return [...items];
  }

  return items
    .map((item, index) => ({
      item,
      index,
      value: value(item),
    }))
    .sort(
      (first, second) =>
        second.value - first.value ||
        first.index - second.index,
    )
    .map(({ item }) => item);
}

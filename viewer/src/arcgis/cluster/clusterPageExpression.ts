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

import { createParquetObjectIdArcadeVariables } from "../parquetObjectId";
import type { ClusterFilePageIndex } from "./clusterPageTopology";

export function createClusterPageValueExpression(
  files: readonly ClusterFilePageIndex[],
  objectIdField: string,
  colorCount: number,
): string {
  const fileExpressions = files.map(
    ({ fileId, pageStarts, rowEnd }) => [
      `if (fileId == ${fileId} && rowId >= ${pageStarts[0]} && rowId < ${rowEnd}) {`,
      `  var pageStarts = [${pageStarts.join(",")}];`,
      "  var low = 0;",
      "  var high = Count(pageStarts) - 1;",
      "  while (low <= high) {",
      "    var middle = Floor((low + high) / 2);",
      "    if (pageStarts[middle] <= rowId) {",
      "      low = middle + 1;",
      "    } else {",
      "      high = middle - 1;",
      "    }",
      "  }",
      `  return high - Floor(high / ${colorCount}) * ${colorCount};`,
      "}",
    ].join("\n"),
  );

  return [
    ...createParquetObjectIdArcadeVariables(objectIdField),
    ...fileExpressions,
    "return -1;",
  ].join("\n");
}

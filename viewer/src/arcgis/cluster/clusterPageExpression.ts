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

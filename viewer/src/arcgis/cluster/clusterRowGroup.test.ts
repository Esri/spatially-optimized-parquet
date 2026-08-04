import { describe, expect, it } from "vitest";

import type { ParquetFileDiagnostics } from "../../parquet/fileLayout";
import { getParquetObjectId } from "../parquetObjectId";
import { resolveClusterRowGroup } from "./clusterRowGroup";

describe("resolveClusterRowGroup", () => {
  it("returns the encoded object ID range containing the hit feature", () => {
    const file = {
      fileId: 2,
      rowGroups: [
        { rowStart: 0, rowCount: 100 },
        { rowStart: 100, rowCount: 50 },
      ],
    } as ParquetFileDiagnostics;

    expect(
      resolveClusterRowGroup([file], getParquetObjectId(2, 125)),
    ).toEqual({
      objectIdStart: getParquetObjectId(2, 100),
      objectIdEnd: getParquetObjectId(2, 150),
    });
  });
});

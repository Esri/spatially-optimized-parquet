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

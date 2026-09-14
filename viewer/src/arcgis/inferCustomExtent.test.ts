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

import type Extent from "@arcgis/core/geometry/Extent";
import { describe, expect, it, vi } from "vitest";

import type {
  ParquetDiagnosticsSnapshot,
  ParquetFileDiagnostics,
} from "../parquet/fileLayout";
import {
  getParquetFileId,
  getParquetObjectId,
  getParquetRowId,
} from "./parquetObjectId";
import {
  inferCustomExtent,
} from "./inferCustomExtent";

describe("inferCustomExtent", () => {
  it("queries the middle row from the row group with the narrowest XZ spread", async () => {
    const extent = {} as Extent;
    const queryFeatures = vi.fn().mockResolvedValue({
      features: [{ geometry: { extent } }],
    });
    const snapshot: ParquetDiagnosticsSnapshot = {
      files: [
        createFile(3, [
          { rowStart: 10, minimum: "100", maximum: "200" },
          { rowStart: 20, minimum: "900719925474099300", maximum: "900719925474099305" },
        ]),
        createFile(7, [
          { rowStart: 30, minimum: "400", maximum: "410" },
        ]),
      ],
    };

    await expect(
      inferCustomExtent({ queryFeatures }, snapshot),
    ).resolves.toBe(extent);
    expect(queryFeatures).toHaveBeenCalledWith({
      objectIds: [getParquetObjectId(3, 25)],
      outFields: [],
      returnGeometry: true,
    });
  });

  it("returns null without querying when XZ statistics are unavailable", async () => {
    const queryFeatures = vi.fn();
    const file = createFile(1, []);
    file.keyValueMetadata = [];

    await expect(
      inferCustomExtent({ queryFeatures }, { files: [file] }),
    ).resolves.toBeNull();
    expect(queryFeatures).not.toHaveBeenCalled();
  });
});

describe("Parquet object IDs", () => {
  it("round trips 16-bit file IDs and 32-bit row IDs losslessly", () => {
    const objectId = getParquetObjectId(15, 160_000_000);

    expect(getParquetFileId(objectId)).toBe(15);
    expect(getParquetRowId(objectId)).toBe(160_000_000);
  });
});

function createFile(
  fileId: number,
  rowGroups: Array<{
    rowStart: number;
    minimum: string;
    maximum: string;
  }>,
): ParquetFileDiagnostics {
  return {
    version: 1,
    fileId,
    fileName: `file-${fileId}.parquet`,
    byteLength: 100,
    footerRange: { start: 90, end: 100 },
    keyValueMetadata: [{
      key: "geodisplay",
      value: JSON.stringify({
        type: "xz",
        code: "indexkey",
      }),
    }],
    columns: [{
      index: 0,
      path: ["indexkey"],
      name: "indexkey",
      physicalType: "int64",
      logicalType: "uint64",
      nullable: false,
      maxDefinitionLevel: 0,
      maxRepetitionLevel: 0,
    }],
    rowGroups: rowGroups.map(({ rowStart, minimum, maximum }, index) => ({
      index,
      rowStart,
      rowCount: 10,
      dataRange: { start: 0, end: 90 },
      bounds: null,
      columns: [{
        columnIndex: 0,
        dataRange: { start: 0, end: 10 },
        columnIndexRange: null,
        offsetIndexRange: null,
        statistics: {
          numValues: 10,
          nullCount: 0,
          distinctCount: null,
          nanCount: null,
          min: { type: "int64", value: minimum },
          max: { type: "int64", value: maximum },
          minExact: true,
          maxExact: true,
        },
        compression: "UNCOMPRESSED",
        encodings: ["PLAIN"],
        compressedSize: 10,
        uncompressedSize: 10,
        dictionaryPageOffset: null,
        dataPageOffset: 0,
      }],
    })),
  };
}

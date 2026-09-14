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
import { deriveClusterLevels } from "./clusterLevelCatalog";

describe("deriveClusterLevels", () => {
  it("maps one logical level to each file's physical column", () => {
    const firstFile = createFile(0, [
      { level: 2, path: ["geodisplay", "level_2"] },
      { level: 16, path: ["geodisplay", "level_16"] },
    ]);
    const secondFile = createFile(1, [
      { level: 2, path: ["geometry", "coarse"] },
      { level: 16, path: ["geometry", "detailed"] },
    ]);

    expect(deriveClusterLevels([firstFile, secondFile])).toEqual([
      {
        level: 2,
        label: "level_2",
        columns: [
          {
            fileId: 0,
            columnIndex: 0,
            fieldName: "geodisplay.level_2",
            repeated: false,
          },
          {
            fileId: 1,
            columnIndex: 0,
            fieldName: "geometry.coarse",
            repeated: false,
          },
        ],
      },
      {
        level: 16,
        label: "level_16",
        columns: [
          {
            fileId: 0,
            columnIndex: 1,
            fieldName: "geodisplay.level_16",
            repeated: false,
          },
          {
            fileId: 1,
            columnIndex: 1,
            fieldName: "geometry.detailed",
            repeated: false,
          },
        ],
      },
    ]);
  });

  it("excludes levels that are unavailable in any dataset file", () => {
    const firstFile = createFile(0, [
      { level: 2, path: ["level_2"] },
      { level: 16, path: ["level_16"] },
    ]);
    const secondFile = createFile(1, [
      { level: 16, path: ["level_16"] },
    ]);

    expect(deriveClusterLevels([firstFile, secondFile]).map(
      ({ level }) => level,
    )).toEqual([16]);
  });

  it("uses the physical X column for native quantized levels", () => {
    const file = createFile(0, [
      { level: 16, path: ["geodisplay", "level_16"] },
    ]);
    file.columns = [
      createColumn(
        0,
        [
          "geodisplay",
          "level_16",
          "list",
          "element",
          "list",
          "element",
          "x",
        ],
      ),
      createColumn(
        1,
        [
          "geodisplay",
          "level_16",
          "list",
          "element",
          "list",
          "element",
          "y",
        ],
      ),
    ];

    expect(deriveClusterLevels([file])).toEqual([
      {
        level: 16,
        label: "level_16",
        columns: [
          {
            fileId: 0,
            columnIndex: 0,
            fieldName:
              "geodisplay.level_16.list.element.list.element.x",
            repeated: true,
          },
        ],
      },
    ]);
  });
});

function createFile(
  fileId: number,
  levels: Array<{ level: number; path: string[] }>,
): ParquetFileDiagnostics {
  return {
    version: 1,
    fileId,
    fileName: `file-${fileId}.parquet`,
    byteLength: 1,
    footerRange: { start: 0, end: 1 },
    keyValueMetadata: [{
      key: "geodisplay",
      value: JSON.stringify({
        levels: levels.map(({ level, path }) => ({ level, column: path })),
      }),
    }],
    columns: levels.map(({ path }, index) => createColumn(index, path)),
    rowGroups: [],
  };
}

function createColumn(index: number, path: string[]) {
  return {
    index,
    path,
    name: path.at(-1)!,
    physicalType: "BYTE_ARRAY",
    logicalType: null,
    nullable: true,
    maxDefinitionLevel: 1,
    maxRepetitionLevel: path.at(-1) === "x" || path.at(-1) === "y" ? 2 : 0,
  };
}

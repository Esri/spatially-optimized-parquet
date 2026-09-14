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

import { describe, expect, it, vi } from "vitest";

import type { ParquetFileDiagnostics } from "../../parquet/fileLayout";
import type { ParquetPageIndexSource } from "../file-explorer/inspector/parquetPageIndexes";
import type { ClusterLevel } from "./clusterLevelCatalog";
import { loadClusterPageTopology } from "./clusterPageTopology";

describe("loadClusterPageTopology", () => {
  it("flattens exact page starts across row groups", async () => {
    const source = createSource([
      {
        pages: [
          createPage(0, 0, 10),
          createPage(1, 10, 20),
        ],
      },
      {
        pages: [
          createPage(0, 0, 5),
          createPage(1, 5, 20),
        ],
      },
    ]);

    await expect(
      loadClusterPageTopology(source, [createFile()], createLevel()),
    ).resolves.toEqual([
      {
        fileId: 0,
        pageStarts: [0, 10, 20, 25],
        rowEnd: 40,
      },
    ]);
  });

  it("rejects missing offset indexes instead of producing partial colors", async () => {
    const source = createSource([null, null]);

    await expect(
      loadClusterPageTopology(source, [createFile()], createLevel()),
    ).rejects.toThrow("Level page offsets are unavailable");
  });

  it("rejects row groups with a gap that would invalidate flat page lookup", async () => {
    const file = createFile();
    file.rowGroups[1].rowStart = 25;
    const source = createSource([
      { pages: [createPage(0, 0, 20)] },
      { pages: [createPage(0, 0, 20)] },
    ]);

    await expect(
      loadClusterPageTopology(source, [file], createLevel()),
    ).rejects.toThrow("Row groups are not contiguous");
  });

  it("collapses repeated X pages that begin within the same feature row", async () => {
    const file = createFile();
    file.rowGroups = [file.rowGroups[0]];
    const source = createSource([
      {
        pages: [
          createPage(0, 0, 0),
          createPage(1, 0, 8),
          createPage(2, 8, 8),
          createPage(3, 8, 20),
        ],
      },
    ]);
    const scalarLevel = createLevel();
    const level = {
      ...scalarLevel,
      columns: scalarLevel.columns.map((column) => ({
        ...column,
        repeated: true,
      })),
    };

    await expect(
      loadClusterPageTopology(source, [file], level),
    ).resolves.toEqual([
      {
        fileId: 0,
        pageStarts: [0, 8],
        rowEnd: 20,
      },
    ]);
  });
});

function createFile(): ParquetFileDiagnostics {
  return {
    version: 1,
    fileId: 0,
    fileName: "example.parquet",
    byteLength: 1,
    footerRange: { start: 0, end: 1 },
    keyValueMetadata: [],
    columns: [],
    rowGroups: [
      {
        index: 0,
        rowStart: 0,
        rowCount: 20,
        dataRange: null,
        bounds: null,
        columns: [],
      },
      {
        index: 1,
        rowStart: 20,
        rowCount: 20,
        dataRange: null,
        bounds: null,
        columns: [],
      },
    ],
  };
}

function createLevel(): ClusterLevel {
  return {
    level: 16,
    label: "level_16",
    columns: [
      {
        fileId: 0,
        columnIndex: 3,
        fieldName: "geodisplay.level_16",
        repeated: false,
      },
    ],
  };
}

function createSource(
  offsetIndexes: Array<Awaited<
    ReturnType<ParquetPageIndexSource["getOffsetIndex"]>
  >>,
): ParquetPageIndexSource {
  const getOffsetIndex = vi.fn();
  for (const offsetIndex of offsetIndexes) {
    getOffsetIndex.mockResolvedValueOnce(offsetIndex);
  }
  return {
    getColumnIndex: vi.fn(),
    getOffsetIndex,
  };
}

function createPage(
  pageIndex: number,
  rowStart: number,
  rowEnd: number,
) {
  return {
    pageIndex,
    rowStart,
    rowEnd,
    byteStart: 0,
    byteEnd: 1,
    compressedPageSize: 1,
  };
}

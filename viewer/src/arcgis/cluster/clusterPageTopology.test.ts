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

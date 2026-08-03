import { describe, expect, it } from "vitest";

import type {
  ParquetDiagnosticsSnapshot,
  ParquetFileDiagnostics,
} from "../../parquet/fileLayout";
import { calculateXZFocusExtent } from "./defaultExtent";
import { approximateBounds, deriveRowGroupBounds } from "./approximateBounds";

describe("deriveRowGroupBounds", () => {
  it("namespaces repeated row-group indexes by diagnostics file", () => {
    const first = createFile(10, "first.parquet", 170, 175);
    const second = createFile(20, "second.parquet", -175, -170);
    const snapshot: ParquetDiagnosticsSnapshot = {
      files: [first, second],
    };

    expect(deriveRowGroupBounds(snapshot, first)[0]).toMatchObject({
      fileId: 10,
      fileName: "first.parquet",
      rowGroupIndex: 0,
    });

    expect(deriveRowGroupBounds(snapshot, second)[0]).toMatchObject({
      fileId: 20,
      fileName: "second.parquet",
      rowGroupIndex: 0,
    });
  });

  describe("approximateBounds", () => {
    it("uses page indexes when the dataset has fewer than four row groups", async () => {
      const file = createXZFile();
      const source = {
        getColumnIndex: async () => ({
          pages: [
            {
              type: "bounds" as const,
              min: { type: "int64" as const, value: "2" },
              max: { type: "int64" as const, value: "2" },
            },
            {
              type: "bounds" as const,
              min: { type: "int64" as const, value: "5" },
              max: { type: "int64" as const, value: "5" },
            },
          ],
        }),
        getOffsetIndex: async () => ({
          pages: [
            createPageLocation(0, 0, 4),
            createPageLocation(1, 4, 10),
          ],
        }),
      };

      await expect(
        approximateBounds({ files: [file] }, source),
      ).resolves.toEqual([
        expect.objectContaining({
          pageIndex: 0,
          featureCount: 4,
          xmin: 0,
          ymin: 0,
          xmax: 4,
          ymax: 4,
        }),
        expect.objectContaining({
          pageIndex: 1,
          featureCount: 6,
          xmin: 2,
          ymin: 2,
          xmax: 6,
          ymax: 6,
        }),
      ]);
    });

    it("keeps row-group bounds when the dataset has at least four row groups", async () => {
      const file = createXZFile();
      file.rowGroups = Array.from({ length: 4 }, (_, index) => ({
        ...file.rowGroups[0],
        index,
        rowStart: index * 10,
      }));
      const source = {
        getColumnIndex: async () => {
          throw new Error("Page indexes should not load.");
        },
        getOffsetIndex: async () => {
          throw new Error("Page indexes should not load.");
        },
      };

      const bounds = await approximateBounds({ files: [file] }, source);

      expect(bounds).toHaveLength(4);
      expect(bounds.every(({ pageIndex }) => pageIndex === null)).toBe(true);
    });

    it("falls back to row-group bounds when page indexes lack complete coverage", async () => {
      const file = createXZFile();
      const source = {
        getColumnIndex: async () => ({
          pages: [{
            type: "bounds" as const,
            min: { type: "int64" as const, value: "2" },
            max: { type: "int64" as const, value: "2" },
          }],
        }),
        getOffsetIndex: async () => ({
          pages: [createPageLocation(0, 0, 4)],
        }),
      };

      const bounds = await approximateBounds({ files: [file] }, source);

      expect(bounds).toEqual([
        expect.objectContaining({
          pageIndex: null,
          featureCount: 10,
        }),
      ]);
    });
  });

  function createXZFile(): ParquetFileDiagnostics {
    const file = createFile(10, "xz.parquet", 0, 8);
    file.keyValueMetadata = [{
      key: "geodisplay",
      value: JSON.stringify({
        type: "xz",
        code: "indexkey",
        maxLevel: 2,
        fullExtent: { xmin: 0, ymin: 0, xmax: 8, ymax: 8 },
      }),
    }];
    file.columns = [{
      index: 0,
      path: ["indexkey"],
      name: "indexkey",
      physicalType: "int64",
      logicalType: "uint64",
      nullable: false,
      maxDefinitionLevel: 0,
      maxRepetitionLevel: 0,
    }];
    file.rowGroups[0] = {
      ...file.rowGroups[0],
      bounds: null,
      columns: [createXZChunk(0, "2", "5")],
    };
    return file;
  }

  function createPageLocation(
    pageIndex: number,
    rowStart: number,
    rowEnd: number,
  ) {
    return {
      pageIndex,
      rowStart,
      rowEnd,
      byteStart: pageIndex * 10,
      byteEnd: pageIndex * 10 + 10,
      compressedPageSize: 10,
    };
  }

  it("combines bounds from multiple files across the antimeridian", () => {
    const first = createFile(10, "first.parquet", 170, 175);
    const second = createFile(20, "second.parquet", -175, -170);
    const snapshot: ParquetDiagnosticsSnapshot = {
      files: [first, second],
    };
    const bounds = [
      ...deriveRowGroupBounds(snapshot, first),
      ...deriveRowGroupBounds(snapshot, second),
    ].map((bound) => ({ ...bound, featureCount: 0 }));

    expect(calculateXZFocusExtent(bounds)?.extent).toEqual({
      xmin: 170,
      xmax: 190,
      ymin: 0,
      ymax: 1,
    });
  });

  it("excludes row groups more than ten times less dense than the median", () => {
    const compactGroups = [
      createRowGroupBounds(0, 100, -100, 30, -99, 31),
      createRowGroupBounds(1, 100, -99, 30, -98, 31),
      createRowGroupBounds(2, 100, -98, 30, -97, 31),
      createRowGroupBounds(3, 100, -97, 30, -96, 31),
    ];
    const worldGroup = createRowGroupBounds(
      4,
      100,
      -179,
      -80,
      179,
      80,
    );

    expect(
      calculateXZFocusExtent([...compactGroups, worldGroup])?.extent,
    ).toEqual({
      xmin: -100,
      ymin: 30,
      xmax: -96,
      ymax: 31,
    });
  });

  it("omits row groups without GeoParquet 2 bounds", () => {
    const file = createFile(10, "geoparquet-1.parquet", 0, 1);
    file.rowGroups.push({
      ...file.rowGroups[0],
      index: 1,
      rowStart: 10,
      bounds: null,
    });
    const snapshot: ParquetDiagnosticsSnapshot = { files: [file] };

    expect(deriveRowGroupBounds(snapshot, file)).toEqual([{
      fileId: 10,
      fileName: "geoparquet-1.parquet",
      rowGroupIndex: 0,
      pageIndex: null,
      featureCount: 10,
      approximate: false,
      xmin: 0,
      xmax: 1,
      ymin: 0,
      ymax: 1,
    }]);
  });

  it("derives approximate bounds from XZ statistics", () => {
    const file = createFile(10, "geoparquet-1.parquet", 0, 1);
    file.keyValueMetadata = [{
      key: "org.apache.spark.sql.parquet.row.metadata",
      value: JSON.stringify({
        fields: [{
          name: "geodisplay",
          metadata: {
            type: "xz",
            code: "indexkey",
            maxLevel: 2,
            fullExtent: { xmin: 0, ymin: 0, xmax: 8, ymax: 8 },
          },
        }],
      }),
    }];
    file.columns = [{
      index: 0,
      path: ["geodisplay", "indexkey"],
      name: "indexkey",
      physicalType: "int64",
      logicalType: "uint64",
      nullable: false,
      maxDefinitionLevel: 0,
      maxRepetitionLevel: 0,
    }];
    file.rowGroups[0] = {
      ...file.rowGroups[0],
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
          min: { type: "int64", value: "2" },
          max: { type: "int64", value: "5" },
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
    };
    const snapshot: ParquetDiagnosticsSnapshot = { files: [file] };

    expect(deriveRowGroupBounds(snapshot, file)).toEqual([{
      fileId: 10,
      fileName: "geoparquet-1.parquet",
      rowGroupIndex: 0,
      pageIndex: null,
      featureCount: 10,
      approximate: true,
      xmin: 0,
      ymin: 0,
      xmax: 6,
      ymax: 6,
    }]);
  });

  it("uses a root SOP cluster-key path without a Spark parent", () => {
    const file = createFile(10, "direct-sop.parquet", 0, 1);
    file.keyValueMetadata = [{
      key: "geodisplay",
      value: JSON.stringify({
        type: "xz",
        code: ["spatial", "cluster_key"],
        maxLevel: 2,
        fullExtent: { xmin: 0, ymin: 0, xmax: 8, ymax: 8 },
      }),
    }];
    file.columns = [{
      index: 0,
      path: ["spatial", "cluster_key"],
      name: "cluster_key",
      physicalType: "int64",
      logicalType: "uint64",
      nullable: false,
      maxDefinitionLevel: 0,
      maxRepetitionLevel: 0,
    }];
    file.rowGroups[0] = {
      ...file.rowGroups[0],
      bounds: null,
      columns: [createXZChunk(0, "2", "5")],
    };
    const snapshot: ParquetDiagnosticsSnapshot = { files: [file] };

    expect(deriveRowGroupBounds(snapshot, file)[0]).toMatchObject({
      approximate: true,
      xmin: 0,
      ymin: 0,
      xmax: 6,
      ymax: 6,
    });
  });

  it("returns no minimap bounds without native or XZ statistics", () => {
    const file = createFile(10, "geoparquet-1.parquet", 0, 1);
    file.rowGroups[0] = { ...file.rowGroups[0], bounds: null };
    const snapshot: ParquetDiagnosticsSnapshot = { files: [file] };

    expect(deriveRowGroupBounds(snapshot, file)).toEqual([]);
  });
});

function createFile(
  fileId: number,
  fileName: string,
  xmin: number,
  xmax: number,
): ParquetFileDiagnostics {
  return {
    version: 1,
    fileId,
    fileName,
    byteLength: 100,
    footerRange: { start: 90, end: 100 },
    keyValueMetadata: [],
    columns: [],
    rowGroups: [{
      index: 0,
      rowStart: 0,
      rowCount: 10,
      dataRange: { start: 0, end: 90 },
      bounds: {
        xmin,
        xmax,
        ymin: 0,
        ymax: 1,
        zmin: null,
        zmax: null,
        mmin: null,
        mmax: null,
      },
      columns: [],
    }],
  };
}

function createXZChunk(
  columnIndex: number,
  minimum: string,
  maximum: string,
): ParquetFileDiagnostics["rowGroups"][number]["columns"][number] {
  return {
    columnIndex,
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
  };
}

function createRowGroupBounds(
  rowGroupIndex: number,
  rowCount: number,
  xmin: number,
  ymin: number,
  xmax: number,
  ymax: number,
) {
  return {
    fileId: 0,
    fileName: "file.parquet",
    rowGroupIndex,
    pageIndex: null,
    featureCount: rowCount,
    approximate: true,
    xmin,
    ymin,
    xmax,
    ymax,
  };
}

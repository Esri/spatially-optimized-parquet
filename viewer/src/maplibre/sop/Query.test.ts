import { describe, expect, it, vi } from "vitest";

import type {
  AsyncBuffer,
  FileMetaData,
  ParquetQueryFilter,
} from "hyparquet";
import { parquetRead } from "hyparquet";

import {
  createXZFilter,
  getTopLevelColumn,
  Query,
} from "./Query";
import type { DatasetParquetMetadata } from "./metadata";
import type { LeafRowReader } from "./PhysicalLeafReader";
import type { RangeReadable } from "./RangeReader";

const file: AsyncBuffer = {
  byteLength: 1,
  slice: async () => new ArrayBuffer(1),
};

const rangeReader: RangeReadable = {
  read: async () => new ArrayBuffer(1),
  asAsyncBuffer: () => file,
  clear: vi.fn(),
};

const metadata: DatasetParquetMetadata = {
  compression: "ZSTD 2x",
  file: {} as FileMetaData,
  display: {
    codePath: ["spatial", "xz"],
    fullExtent: { xmin: 0, ymin: 0, xmax: 10, ymax: 10 },
    geometryType: "polyline",
    maxLevel: 1,
    levels: [
      {
        columnPath: ["geolod", "level_0"],
        level: 0,
        resolution: 1,
        transform: {
          scale: [1, 1, 1, 1],
          translate: [0, 0, 0, 0],
        },
      },
    ],
    version: "0.1",
  },
};

describe("createXZFilter", () => {
  it("creates one inclusive predicate for one range", () => {
    expect(
      createXZFilter(["spatial", "xz"], [{ start: 4, end: 9 }]),
    ).toEqual({
      "spatial.xz": {
        $gte: 4,
        $lte: 9,
      },
    });
  });

  it("creates a disjunction for multiple ranges", () => {
    expect(
      createXZFilter(
        ["xz"],
        [
          { start: 1, end: 2 },
          { start: 8, end: 13 },
        ],
      ),
    ).toEqual({
      $or: [
        { xz: { $gte: 1, $lte: 2 } },
        { xz: { $gte: 8, $lte: 13 } },
      ],
    });
  });

  it("rejects an empty range set", () => {
    expect(() => createXZFilter(["xz"], [])).toThrow(
      "XZ filter requires at least one range",
    );
  });
});

describe("column paths", () => {
  it("projects the top-level owner", () => {
    expect(getTopLevelColumn(["geolod", "level_0"])).toBe("geolod");
    expect(() => getTopLevelColumn([])).toThrow(
      "Parquet column path must not be empty",
    );
  });
});

describe("Query", () => {
  it("scans XZ pages before reading one physical LOD leaf", async () => {
    const parquetReader = vi.fn<typeof parquetRead>(async (options) => {
      options.onPage?.({
        pathInSchema: ["spatial", "xz"],
        columnData: BigInt64Array.of(0n, 4n),
        rowStart: 12,
        rowEnd: 14,
      });
    });
    const geometry = Uint8Array.of(
      0x12,
      0x01,
      0x02,
      0x1a,
      0x04,
      0x02,
      0x02,
      0x02,
      0x00,
    );
    const leafReader: LeafRowReader = {
      readRows: vi.fn(async () => new Map([[12, geometry]])),
    };
    const query = new Query("unused", file.byteLength, {
      leafReader,
      metadataLoader: async () => metadata,
      parquetReader,
      rangeReader,
    });
    const signal = new AbortController().signal;

    const result = await query.execute({
      queryExtents: [{ xmin: 0, ymin: 0, xmax: 5, ymax: 5 }],
      sourceResolution: 1,
      signal,
    });

    expect(parquetReader).toHaveBeenCalledOnce();
    const options = parquetReader.mock.calls[0][0];
    expect(options).toMatchObject({
      columns: ["spatial"],
      metadata: metadata.file,
      rowFormat: "object",
      usePageIndex: true,
    });
    expect(options.filter as ParquetQueryFilter).toEqual({
      "spatial.xz": { $gte: 0, $lte: 1 },
    });
    expect(leafReader.readRows).toHaveBeenCalledWith(
      ["geolod", "level_0"],
      [12],
      signal,
    );
    expect(result).toEqual({
      compression: "ZSTD 2x",
      featureCollection: {
        type: "FeatureCollection",
        features: [
          {
            type: "Feature",
            properties: {},
            geometry: {
              type: "LineString",
              coordinates: [
                [1, 1],
                [2, 1],
              ],
            },
          },
        ],
      },
      featureLimitReached: false,
      geometryType: "polyline",
      lod: 0,
    });
  });

  it("does not query Parquet for a disjoint viewport", async () => {
    const parquetReader = vi.fn<typeof parquetRead>();
    const leafReader: LeafRowReader = {
      readRows: vi.fn(async () => new Map()),
    };
    const query = new Query("unused", file.byteLength, {
      leafReader,
      metadataLoader: async () => metadata,
      parquetReader,
      rangeReader,
    });

    const result = await query.execute({
      queryExtents: [{ xmin: -5, ymin: -5, xmax: -1, ymax: -1 }],
      sourceResolution: 1,
      signal: new AbortController().signal,
    });

    expect(parquetReader).not.toHaveBeenCalled();
    expect(leafReader.readRows).not.toHaveBeenCalled();
    expect(result.featureCollection.features).toEqual([]);
  });
});

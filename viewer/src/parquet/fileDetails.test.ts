import { describe, expect, it } from "vitest";

import {
  deriveDatasetDetailSummary,
  deriveFileDetailSummary,
} from "./fileDetails";
import type { ColumnLayout, FileLayout } from "./fileLayout";

function createColumn(
  rowGroupIndex: number,
  recordCount: number,
  compression: string,
  compressedSize: number,
  uncompressedSize: number,
): ColumnLayout {
  return {
    id: `rg${rowGroupIndex}-c0`,
    rowGroupIndex,
    columnIndex: 0,
    fieldName: "value",
    byteRange: { start: rowGroupIndex * 100, end: rowGroupIndex * 100 + 50 },
    minimumValue: null,
    maximumValue: null,
    nullCount: null,
    recordCount,
    valueCount: recordCount,
    physicalType: "INT32",
    logicalType: null,
    nullable: true,
    maxDefinitionLevel: 1,
    maxRepetitionLevel: 0,
    compression,
    encodings: ["PLAIN"],
    compressedSize,
    uncompressedSize,
    dictionaryPageOffset: null,
    dataPageOffset: rowGroupIndex * 100,
  };
}

describe("deriveFileDetailSummary", () => {
  it("derives rows, compression, and sizes from the file layout", () => {
    const layout: FileLayout = {
      fileId: 0,
      fileName: "file.parquet",
      byteLength: 900,
      footer: { start: 800, end: 900 },
      keyValueMetadata: [],
      rowGroups: [
        {
          index: 0,
          byteRange: { start: 0, end: 50 },
          columns: [createColumn(0, 2, "GZIP", 50, 100)],
        },
        {
          index: 1,
          byteRange: { start: 100, end: 150 },
          columns: [createColumn(1, 3, "SNAPPY", 75, 150)],
        },
      ],
      pageIndexes: [],
    };

    expect(deriveFileDetailSummary(layout)).toMatchObject({
      rowCount: 5,
      compressionCodecs: ["GZIP", "SNAPPY"],
      compressedSize: 125,
      uncompressedSize: 250,
    });
    expect(layout.byteLength).toBe(900);
  });
});

describe("deriveDatasetDetailSummary", () => {
  it("aggregates rows, physical bytes, columns, and compression across files", () => {
    const firstLayout = createLayout(
      0,
      "first.parquet",
      900,
      [createColumn(0, 2, "gzip", 50, 100)],
    );
    const secondLayout = createLayout(
      1,
      "second.parquet",
      1_100,
      [
        createColumn(0, 3, "SNAPPY", 75, 225),
        {
          ...createColumn(0, 3, "GZIP", 25, 75),
          columnIndex: 1,
          fieldName: "category",
          id: "rg0-c1",
        },
      ],
    );

    expect(deriveDatasetDetailSummary([firstLayout, secondLayout])).toEqual({
      byteLength: 2_000,
      fileCount: 2,
      rowCount: 5,
      columnCount: 2,
      rowGroupCount: 2,
      compressionCodecs: ["GZIP", "SNAPPY"],
      compressedSize: 150,
      uncompressedSize: 400,
    });
  });
});

function createLayout(
  fileId: number,
  fileName: string,
  byteLength: number,
  columns: ColumnLayout[],
): FileLayout {
  return {
    fileId,
    fileName,
    byteLength,
    footer: { start: byteLength - 100, end: byteLength },
    keyValueMetadata: [],
    rowGroups: [{
      index: 0,
      byteRange: { start: 0, end: 100 },
      columns,
    }],
    pageIndexes: [],
  };
}

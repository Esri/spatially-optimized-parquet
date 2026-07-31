import { describe, expect, it } from "vitest";

import { deriveFileDetailSummary } from "./fileDetails";
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

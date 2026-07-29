import { describe, expect, it } from "vitest";

import { deriveFileDetailSummary } from "./parquetFileDetails";
import type { ColumnLayout, FileLayout } from "./parquetFileLayout";

describe("deriveFileDetailSummary", () => {
  it("aggregates file-level counts, compression, indexes, and bounds", () => {
    const firstValue = createColumn(0, 0, "value", "gzip", 100, 250, 10);
    const firstGeometry = createColumn(0, 1, "geometry", "zstd", 200, 600, 10);
    const secondValue = createColumn(1, 0, "value", "gzip", 120, 300, 15);
    const secondGeometry = createColumn(1, 1, "geometry", "zstd", 180, 550, 15);
    const layout: FileLayout = {
      fileId: "file",
      fileName: "file.parquet",
      byteLength: 1_000,
      footer: { start: 900, end: 1_000 },
      keyValueMetadata: [],
      rowGroups: [
        {
          index: 0,
          byteRange: { start: 0, end: 300 },
          columns: [firstValue, firstGeometry],
        },
        {
          index: 1,
          byteRange: { start: 300, end: 700 },
          columns: [secondValue, secondGeometry],
        },
      ],
      pageIndexes: [
        {
          id: "column-index",
          rowGroupIndex: 0,
          fieldName: "value",
          kind: "column",
          byteRange: { start: 700, end: 710 },
          minimumValue: null,
          maximumValue: null,
          nullCount: null,
          recordCount: 10,
        },
        {
          id: "offset-index",
          rowGroupIndex: 0,
          fieldName: "value",
          kind: "offset",
          byteRange: { start: 710, end: 720 },
          minimumValue: null,
          maximumValue: null,
          nullCount: null,
          recordCount: 10,
        },
      ],
    };

    expect(deriveFileDetailSummary(layout)).toEqual({
      rowCount: 25,
      columnCount: 2,
      rowGroupCount: 2,
      compressionCodecs: ["gzip", "zstd"],
      compressedSize: 600,
      uncompressedSize: 1_700,
    });
  });
});

function createColumn(
  rowGroupIndex: number,
  columnIndex: number,
  fieldName: string,
  compression: string,
  compressedSize: number,
  uncompressedSize: number,
  recordCount: number,
): ColumnLayout {
  return {
    id: `rg${rowGroupIndex}-c${columnIndex}`,
    rowGroupIndex,
    columnIndex,
    fieldName,
    byteRange: { start: 0, end: compressedSize },
    minimumValue: null,
    maximumValue: null,
    nullCount: null,
    recordCount,
    valueCount: 1,
    physicalType: "BYTE_ARRAY",
    logicalType: null,
    nullable: true,
    maxDefinitionLevel: 1,
    maxRepetitionLevel: 0,
    compression,
    encodings: ["PLAIN"],
    compressedSize,
    uncompressedSize,
    dictionaryPageOffset: null,
    dataPageOffset: 0,
  };
}

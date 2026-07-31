import { describe, expect, it } from "vitest";

import { createDownloadDisplayLayout, displaySubpartCount } from "./displayLayout";
import type { FileLayout } from "./fileLayout";

describe("createDownloadDisplayLayout", () => {
  it("concatenates column chunks into blocks that retain physical pieces", () => {
    const layout: FileLayout = {
      fileId: 0,
      fileName: "file.parquet",
      byteLength: 3_000_000,
      footer: { start: 2_900_000, end: 3_000_000 },
      keyValueMetadata: [],
      rowGroups: [
        {
          index: 0,
          byteRange: { start: 100, end: 700_000 },
          columns: [{
            id: "rg0-c0",
            rowGroupIndex: 0,
            columnIndex: 0,
            fieldName: "value",
            byteRange: { start: 100, end: 700_000 },
            minimumValue: null,
            maximumValue: null,
            nullCount: null,
            recordCount: 1,
            valueCount: 1,
            physicalType: "INT32",
            logicalType: null,
            nullable: true,
            maxDefinitionLevel: 1,
            maxRepetitionLevel: 0,
            compression: "GZIP",
            encodings: ["PLAIN"],
            compressedSize: 699_900,
            uncompressedSize: 1_000_000,
            dictionaryPageOffset: null,
            dataPageOffset: 100,
          }],
        },
        {
          index: 1,
          byteRange: { start: 1_000_000, end: 1_700_000 },
          columns: [{
            id: "rg1-c0",
            rowGroupIndex: 1,
            columnIndex: 0,
            fieldName: "value",
            byteRange: { start: 1_000_000, end: 1_700_000 },
            minimumValue: null,
            maximumValue: null,
            nullCount: null,
            recordCount: 1,
            valueCount: 1,
            physicalType: "INT32",
            logicalType: null,
            nullable: true,
            maxDefinitionLevel: 1,
            maxRepetitionLevel: 0,
            compression: "GZIP",
            encodings: ["PLAIN"],
            compressedSize: 699_900,
            uncompressedSize: 1_000_000,
            dictionaryPageOffset: null,
            dataPageOffset: 700_100,
          }],
        },
      ],
      pageIndexes: [],
    };

    const display = createDownloadDisplayLayout(layout);
    const column = display.tracks.find((track) => track.id === "column:value");
    expect(column?.blocks).toHaveLength(1);
    expect(column?.blocks[0].logicalRange).toEqual({ start: 0, end: 1_399_900 });
    expect(column?.blocks[0].physicalPieces).toHaveLength(2);
    expect(column?.blocks[0].subparts).toHaveLength(displaySubpartCount);
  });
});

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

import {
  createDatasetDownloadDisplayLayout,
  createDownloadDisplayLayout,
  displaySubpartCount,
} from "./displayLayout";
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

describe("createDatasetDownloadDisplayLayout", () => {
  it("aggregates page-index segments without variadic stack growth", () => {
    const pageIndexCount = 150_000;
    const layout: FileLayout = {
      fileId: 0,
      fileName: "large.parquet",
      byteLength: 1,
      footer: { start: 0, end: 1 },
      keyValueMetadata: [],
      rowGroups: [{
        index: 0,
        byteRange: { start: 0, end: 0 },
        columns: [],
      }],
      pageIndexes: Array.from({ length: pageIndexCount }, (_, index) => ({
        id: `page-${index}`,
        rowGroupIndex: 0,
        fieldName: "geometry",
        kind: "column" as const,
        byteRange: { start: 0, end: 0 },
        minimumValue: null,
        maximumValue: null,
        nullCount: null,
        recordCount: 0,
      })),
    };

    const display = createDatasetDownloadDisplayLayout([layout]);
    const pageIndexTrack = display.displayLayout.tracks.find(
      ({ id }) => id === "page-index",
    );

    expect(pageIndexTrack?.segments).toHaveLength(pageIndexCount);
  });
});

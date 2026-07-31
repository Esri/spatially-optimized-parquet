import { describe, expect, it, vi } from "vitest";

import { ParquetByteCoverage } from "../../../parquet/ParquetByteCoverage";
import { ParquetFileStructureStore } from "./ParquetFileStructureStore";

describe("ParquetFileStructureStore", () => {
  it("selects and loads the first column when opening a row group", async () => {
    const source = {
      getColumnIndex: vi.fn().mockResolvedValue({
        pages: [{ type: "bounds", min: { type: "int32", value: 1 }, max: { type: "int32", value: 2 } }],
      }),
      getOffsetIndex: vi.fn().mockResolvedValue({
        pages: [{
          pageIndex: 0,
          rowStart: 0,
          rowEnd: 10,
          byteStart: 100,
          byteEnd: 150,
          compressedPageSize: 50,
        }],
      }),
    };
    const coverage = new ParquetByteCoverage();
    const column = {
      id: "rg0-c0",
      rowGroupIndex: 0,
      columnIndex: 0,
      fieldName: "value",
      byteRange: { start: 100, end: 200 },
      minimumValue: 1,
      maximumValue: 2,
      nullCount: 0,
      recordCount: 10,
      valueCount: 10,
      physicalType: "INT32",
      logicalType: null,
      nullable: true,
      maxDefinitionLevel: 1,
      maxRepetitionLevel: 0,
      compression: "GZIP",
      encodings: ["PLAIN"],
      compressedSize: 100,
      uncompressedSize: 200,
      dictionaryPageOffset: null,
      dataPageOffset: 100,
    };
    const store = new ParquetFileStructureStore({
      coverage,
      layout: {
        fileId: 0,
        fileName: "file.parquet",
        byteLength: 300,
        footer: { start: 280, end: 300 },
        keyValueMetadata: [],
        rowGroups: [{
          index: 0,
          byteRange: { start: 100, end: 200 },
          columns: [column],
        }],
        pageIndexes: [
          {
            id: "column-index",
            rowGroupIndex: 0,
            fieldName: "value",
            kind: "column",
            byteRange: { start: 200, end: 220 },
            minimumValue: 1,
            maximumValue: 2,
            nullCount: 0,
            recordCount: 10,
          },
          {
            id: "offset-index",
            rowGroupIndex: 0,
            fieldName: "value",
            kind: "offset",
            byteRange: { start: 220, end: 240 },
            minimumValue: 1,
            maximumValue: 2,
            nullCount: 0,
            recordCount: 10,
          },
        ],
      },
    }, source);

    store.selectRowGroup(0);
    expect(store.getState().expandedColumnIds).toEqual(new Set([column.id]));
    await vi.waitFor(() => {
      expect(store.getState().details.get(column.id)?.type).toBe("ready");
    });

    expect(source.getColumnIndex).toHaveBeenCalledOnce();
    expect(source.getOffsetIndex).toHaveBeenCalledOnce();
    expect(coverage.state({ start: 200, end: 240 })).toBe("loaded");
  });

  it("keeps the current details visible until the next column loads", async () => {
    const firstColumn = createColumn("first", 0, { start: 0, end: 100 });
    const secondColumn = createColumn("second", 1, { start: 100, end: 200 });
    let resolveSecondOffsetIndex:
      | ((value: {
          pages: {
            pageIndex: number;
            rowStart: number;
            rowEnd: number;
            byteStart: number;
            byteEnd: number;
            compressedPageSize: number;
          }[];
        }) => void)
      | undefined;
    const secondOffsetIndex = new Promise<{
      pages: {
        pageIndex: number;
        rowStart: number;
        rowEnd: number;
        byteStart: number;
        byteEnd: number;
        compressedPageSize: number;
      }[];
    }>((resolve) => {
      resolveSecondOffsetIndex = resolve;
    });
    const source = {
      getColumnIndex: vi.fn().mockResolvedValue(null),
      getOffsetIndex: vi
        .fn()
        .mockResolvedValueOnce({ pages: [] })
        .mockReturnValueOnce(secondOffsetIndex),
    };
    const store = new ParquetFileStructureStore({
      coverage: new ParquetByteCoverage(),
      layout: {
        fileId: 0,
        fileName: "file.parquet",
        byteLength: 300,
        footer: { start: 280, end: 300 },
        keyValueMetadata: [],
        rowGroups: [{
          index: 0,
          byteRange: { start: 0, end: 200 },
          columns: [firstColumn, secondColumn],
        }],
        pageIndexes: [],
      },
    }, source);

    store.selectRowGroup(0);
    await vi.waitFor(() => {
      expect(store.getState().detailColumnId).toBe(firstColumn.id);
    });

    store.toggleColumn(secondColumn);

    expect(store.getState().expandedColumnIds).toEqual(
      new Set([secondColumn.id]),
    );
    expect(store.getState().detailColumnId).toBe(firstColumn.id);

    resolveSecondOffsetIndex?.({ pages: [] });
    await vi.waitFor(() => {
      expect(store.getState().detailColumnId).toBe(secondColumn.id);
    });
  });
});

function createColumn(
  fieldName: string,
  columnIndex: number,
  byteRange: { start: number; end: number },
) {
  return {
    id: `rg0-c${columnIndex}`,
    rowGroupIndex: 0,
    columnIndex,
    fieldName,
    byteRange,
    minimumValue: null,
    maximumValue: null,
    nullCount: null,
    recordCount: 10,
    valueCount: 10,
    physicalType: "INT32",
    logicalType: null,
    nullable: true,
    maxDefinitionLevel: 1,
    maxRepetitionLevel: 0,
    compression: "GZIP",
    encodings: ["PLAIN"],
    compressedSize: byteRange.end - byteRange.start,
    uncompressedSize: byteRange.end - byteRange.start,
    dictionaryPageOffset: null,
    dataPageOffset: byteRange.start,
  };
}

import { afterEach, describe, expect, it, vi } from "vitest";

import type {
  ArcgisParquetDiagnosticsSnapshotV1,
  ArcgisParquetRangeReadCompleteEvent,
  ArcgisParquetRangeReadStartEvent,
} from "./arcgisParquetDiagnostics";
import { ParquetDownloadStore } from "./parquetDownloadStore";

const diagnostics: ArcgisParquetDiagnosticsSnapshotV1 = {
  files: [{
    version: 1,
    fileId: "file",
    fileName: "file.parquet",
    byteLength: 2_000_000,
    footerRange: { start: 1_900_000, end: 2_000_000 },
    keyValueMetadata: [],
    columns: [{
      index: 0,
      path: ["value"],
      name: "value",
      physicalType: "INT32",
      logicalType: null,
      nullable: true,
      maxDefinitionLevel: 1,
      maxRepetitionLevel: 0,
    }],
    rowGroups: [{
      index: 0,
      rowStart: 0,
      rowCount: 1,
      dataRange: { start: 0, end: 1_000_000 },
      bounds: {
        xmin: 0,
        xmax: 1,
        ymin: 0,
        ymax: 1,
        zmin: null,
        zmax: null,
        mmin: null,
        mmax: null,
      },
      columns: [{
        columnIndex: 0,
        dataRange: { start: 0, end: 1_000_000 },
        columnIndexRange: { start: 1_000_000, end: 1_000_100 },
        offsetIndexRange: { start: 1_000_100, end: 1_000_300 },
        compression: "GZIP",
        encodings: ["PLAIN"],
        compressedSize: 1_000_000,
        uncompressedSize: 2_000_000,
        dictionaryPageOffset: null,
        dataPageOffset: 0,
        statistics: {
          numValues: 1,
          nullCount: 2,
          distinctCount: null,
          nanCount: null,
          min: { type: "int32", value: 10 },
          max: { type: "int32", value: 20 },
          minExact: null,
          maxExact: null,
        },
      }],
    }],
  }],
};

describe("ParquetDownloadStore", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("fills only the affected subpart and batches request updates for 64 ms", () => {
    vi.useFakeTimers();
    const store = new ParquetDownloadStore();
    store.setDiagnostics(diagnostics);
    const blockId = "column:value:block:0";
    const listener = vi.fn();
    store.subscribeBlock(blockId, listener);
    const start: ArcgisParquetRangeReadStartEvent = {
      phase: "start",
      fileId: "file",
      requestId: 1,
      range: { start: 0, end: 1 },
    };
    const complete: ArcgisParquetRangeReadCompleteEvent = {
      ...start,
      phase: "complete",
    };

    store.handleRangeRead(start);
    store.handleRangeRead(complete);
    expect(listener).not.toHaveBeenCalled();
    vi.advanceTimersByTime(64);

    expect(listener).toHaveBeenCalledTimes(1);
    expect(store.getBlockSnapshot(blockId).cachedMask).toBe(1);
    expect(store.getSummarySnapshot().downloadedByteLength).toBe(1);
    const [rowGroupCoverage] = store.getTrackRowGroupCoverage("column:value");
    expect(rowGroupCoverage.rowGroupIndex).toBe(0);
    expect(rowGroupCoverage.downloadedByteLength).toBe(1);
    expect(rowGroupCoverage.byteLength).toBe(1_000_000);
    expect(rowGroupCoverage.downloadedPercent).toBeCloseTo(0.0001);
    expect(rowGroupCoverage.statistics).toEqual({
      minimumValue: 10,
      maximumValue: 20,
      nullCount: 2,
      recordCount: 1,
    });
    expect(rowGroupCoverage.itemCoverage).toEqual([]);
    expect(store.getTrackRowGroupBlockMasks("column:value", 0).get(blockId)).toBe(1);
  });

  it("aggregates all page-index bytes for each row group", () => {
    const store = new ParquetDownloadStore();
    store.setDiagnostics(diagnostics);
    const complete: ArcgisParquetRangeReadCompleteEvent = {
      phase: "complete",
      fileId: "file",
      requestId: 2,
      range: { start: 1_000_000, end: 1_000_100 },
    };

    store.handleRangeRead(complete);

    const [rowGroupCoverage] = store.getTrackRowGroupCoverage("page-index");
    expect(rowGroupCoverage.rowGroupIndex).toBe(0);
    expect(rowGroupCoverage.downloadedByteLength).toBe(100);
    expect(rowGroupCoverage.byteLength).toBe(300);
    expect(rowGroupCoverage.downloadedPercent).toBeCloseTo(100 / 3);
    expect(rowGroupCoverage.itemCoverage).toEqual([{
      label: "value",
      downloadedByteLength: 100,
      byteLength: 300,
      downloaded: false,
      minimumValue: 10,
      maximumValue: 20,
      nullCount: 2,
      recordCount: 1,
    }]);

    store.handleRangeRead({
      ...complete,
      requestId: 3,
      range: { start: 1_000_100, end: 1_000_300 },
    });

    const [completedRowGroupCoverage] = store.getTrackRowGroupCoverage("page-index");
    expect(completedRowGroupCoverage.itemCoverage).toEqual([{
      label: "value",
      downloadedByteLength: 300,
      byteLength: 300,
      downloaded: true,
      minimumValue: 10,
      maximumValue: 20,
      nullCount: 2,
      recordCount: 1,
    }]);
  });
});

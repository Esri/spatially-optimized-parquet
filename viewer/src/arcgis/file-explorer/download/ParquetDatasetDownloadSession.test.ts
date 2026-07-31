import { afterEach, describe, expect, it, vi } from "vitest";

import type {
  ArcgisParquetDiagnosticsSnapshotV1,
  ArcgisParquetFileDiagnosticsV1,
} from "../../diagnostics";
import { ParquetDatasetDownloadSession } from "./ParquetDatasetDownloadSession";

describe("ParquetDatasetDownloadSession", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("isolates overlapping byte ranges by diagnostics file", () => {
    const session = new ParquetDatasetDownloadSession();
    session.loadDiagnostics(createDiagnostics());

    expect(session.detailSummary).toMatchObject({
      byteLength: 3_000,
      fileCount: 2,
      rowCount: 5,
      compressionCodecs: ["GZIP", "SNAPPY"],
    });
    expect(session.rowGroupBounds).toHaveLength(2);

    session.recordRangeRead({
      phase: "complete",
      fileId: "first.parquet",
      requestId: 1,
      range: { start: 0, end: 50 },
    });

    const [first, second] = session.files;
    expect(
      first.download.createFileStructureSnapshot()?.coverage.coveredByteLength({
        start: 0,
        end: 50,
      }),
    ).toBe(50);
    expect(
      second.download.createFileStructureSnapshot()?.coverage.coveredByteLength({
        start: 0,
        end: 50,
      }),
    ).toBe(0);
  });

  it("replays range events received before diagnostics initialization", () => {
    const session = new ParquetDatasetDownloadSession();
    session.recordRangeRead({
      phase: "complete",
      fileId: "second.parquet",
      requestId: 1,
      range: { start: 10, end: 40 },
    });

    session.loadDiagnostics(createDiagnostics());

    expect(
      session.files[1].download
        .createFileStructureSnapshot()
        ?.coverage.coveredByteLength({ start: 10, end: 40 }),
    ).toBe(30);
  });

  it("generates column blocks from combined bytes across every file", () => {
    vi.useFakeTimers();
    const session = new ParquetDatasetDownloadSession();
    session.loadDiagnostics(createDiagnostics());
    const aggregate = session.aggregateDownload;
    const columnTrack = aggregate.topology
      .getSnapshot()
      .tracks
      .find(({ id }) => id === "column:value");

    expect(columnTrack?.byteLength).toBe(1_500);
    expect(columnTrack?.blocks).toHaveLength(1);

    session.recordRangeRead({
      phase: "complete",
      fileId: "first.parquet",
      requestId: 1,
      range: { start: 0, end: 50 },
    });
    session.recordRangeRead({
      phase: "complete",
      fileId: "second.parquet",
      requestId: 1,
      range: { start: 0, end: 50 },
    });
    vi.advanceTimersByTime(64);

    expect(aggregate.summary.getSnapshot().downloadedByteLength).toBe(100);
    expect(aggregate.track("column:value").getSnapshot()).toMatchObject({
      downloadedByteLength: 100,
      visibleBlockCount: 1,
      visibleBlockIds: ["column:value:block:0"],
    });
    expect(aggregate.rowGroupCoverage("column:value")).toMatchObject([
      {
        fileId: 0,
        fileName: "first.parquet",
        sourceRowGroupIndex: 0,
      },
      {
        fileId: 1,
        fileName: "second.parquet",
        sourceRowGroupIndex: 0,
      },
    ]);
  });

  it("reports unknown files without contaminating any child coverage", () => {
    vi.useFakeTimers();
    const session = new ParquetDatasetDownloadSession();
    session.loadDiagnostics(createDiagnostics());

    session.recordRangeRead({
      phase: "complete",
      fileId: "unknown.parquet",
      requestId: 1,
      range: { start: 0, end: 50 },
    });
    vi.advanceTimersByTime(64);

    for (const file of session.files) {
      expect(file.download.summary.getSnapshot().downloadedByteLength).toBe(0);
      expect(file.download.topology.getSnapshot().error?.message).toContain(
        'unknown diagnostics file "unknown.parquet"',
      );
    }
  });

  it("rejects duplicate event-routing filenames", () => {
    const session = new ParquetDatasetDownloadSession();
    const diagnostics = createDiagnostics();
    diagnostics.files[1] = {
      ...diagnostics.files[1],
      fileName: diagnostics.files[0].fileName,
    };

    expect(() => session.loadDiagnostics(diagnostics)).toThrow(
      'duplicate fileName "first.parquet"',
    );
    expect(session.files).toHaveLength(0);
  });

  it("disposes every child session", () => {
    const session = new ParquetDatasetDownloadSession();
    session.loadDiagnostics(createDiagnostics());
    const childSessions = session.files.map(({ download }) => download);

    session.dispose();

    expect(session.files).toHaveLength(0);
    for (const childSession of childSessions) {
      expect(childSession.topology.getSnapshot().layout).toBeNull();
    }
  });

  it("loads file diagnostics when GeoParquet row-group bounds are unavailable", () => {
    const session = new ParquetDatasetDownloadSession();
    const diagnostics = createDiagnostics();
    diagnostics.files = diagnostics.files.map((file) => ({
      ...file,
      rowGroups: file.rowGroups.map((rowGroup) => ({
        ...rowGroup,
        bounds: null,
      })),
    }));

    session.loadDiagnostics(diagnostics);

    expect(session.files).toHaveLength(2);
    expect(session.detailSummary?.rowCount).toBe(5);
    expect(session.rowGroupBounds).toBeNull();
  });
});

function createDiagnostics(): ArcgisParquetDiagnosticsSnapshotV1 {
  return {
    files: [
      createFile(0, "first.parquet", 1_000, 2, "GZIP"),
      createFile(1, "second.parquet", 2_000, 3, "SNAPPY"),
    ],
  };
}

function createFile(
  fileId: number,
  fileName: string,
  byteLength: number,
  rowCount: number,
  compression: string,
): ArcgisParquetFileDiagnosticsV1 {
  const dataEnd = Math.floor(byteLength / 2);
  return {
    version: 1,
    fileId,
    fileName,
    byteLength,
    footerRange: { start: byteLength - 100, end: byteLength },
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
      rowCount,
      dataRange: { start: 0, end: dataEnd },
      bounds: {
        xmin: fileId,
        xmax: fileId + 1,
        ymin: 0,
        ymax: 1,
        zmin: null,
        zmax: null,
        mmin: null,
        mmax: null,
      },
      columns: [{
        columnIndex: 0,
        dataRange: { start: 0, end: dataEnd },
        columnIndexRange: null,
        offsetIndexRange: null,
        compression,
        encodings: ["PLAIN"],
        compressedSize: dataEnd,
        uncompressedSize: dataEnd * 2,
        dictionaryPageOffset: null,
        dataPageOffset: 0,
        statistics: {
          numValues: rowCount,
          nullCount: 0,
          distinctCount: null,
          nanCount: null,
          min: null,
          max: null,
          minExact: null,
          maxExact: null,
        },
      }],
    }],
  };
}

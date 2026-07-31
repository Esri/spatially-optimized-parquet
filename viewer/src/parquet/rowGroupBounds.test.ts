import { describe, expect, it } from "vitest";

import type {
  ArcgisParquetDiagnosticsSnapshotV1,
  ArcgisParquetFileDiagnosticsV1,
} from "./fileLayout";
import {
  calculateRowGroupFocusExtent,
  deriveRowGroupBounds,
} from "./rowGroupBounds";

describe("deriveRowGroupBounds", () => {
  it("namespaces repeated row-group indexes by diagnostics file", () => {
    const first = createFile(10, "first.parquet", 170, 175);
    const second = createFile(20, "second.parquet", -175, -170);
    const snapshot: ArcgisParquetDiagnosticsSnapshotV1 = {
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

  it("combines bounds from multiple files across the antimeridian", () => {
    const first = createFile(10, "first.parquet", 170, 175);
    const second = createFile(20, "second.parquet", -175, -170);
    const snapshot: ArcgisParquetDiagnosticsSnapshotV1 = {
      files: [first, second],
    };
    const bounds = [
      ...deriveRowGroupBounds(snapshot, first),
      ...deriveRowGroupBounds(snapshot, second),
    ].map((bound) => ({ ...bound, rowCount: 0 }));

    expect(calculateRowGroupFocusExtent(bounds)).toEqual({
      xmin: 170,
      xmax: 190,
      ymin: 0,
      ymax: 1,
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
    const snapshot: ArcgisParquetDiagnosticsSnapshotV1 = { files: [file] };

    expect(deriveRowGroupBounds(snapshot, file)).toEqual([{
      fileId: 10,
      fileName: "geoparquet-1.parquet",
      rowGroupIndex: 0,
      rowCount: 10,
      xmin: 0,
      xmax: 1,
      ymin: 0,
      ymax: 1,
    }]);
  });

  it("returns no minimap bounds for files without GeoParquet 2 metadata", () => {
    const file = createFile(10, "geoparquet-1.parquet", 0, 1);
    file.rowGroups[0] = {
      ...file.rowGroups[0],
      bounds: null,
    };
    const snapshot: ArcgisParquetDiagnosticsSnapshotV1 = { files: [file] };

    expect(deriveRowGroupBounds(snapshot, file)).toEqual([]);
  });
});

function createFile(
  fileId: number,
  fileName: string,
  xmin: number,
  xmax: number,
): ArcgisParquetFileDiagnosticsV1 {
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

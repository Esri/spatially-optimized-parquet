import { describe, expect, it } from "vitest";

import type {
  ParquetDiagnosticsSnapshot,
  ParquetFileDiagnostics,
} from "../parquet/fileLayout";
import { deriveRowGroupBounds } from "./rowGroupBounds";

describe("deriveRowGroupBounds", () => {
  it("returns native row-group bounds with file identity", () => {
    const file = createFile();
    const snapshot: ParquetDiagnosticsSnapshot = { files: [file] };

    expect(deriveRowGroupBounds(snapshot, file)).toEqual([{
      fileId: 10,
      fileName: "file.parquet",
      rowGroupIndex: 0,
      featureCount: 10,
      xmin: 0,
      ymin: 1,
      xmax: 2,
      ymax: 3,
    }]);
  });

  it("does not approximate missing bounds from XZ statistics", () => {
    const file = createFile();
    file.keyValueMetadata = [{
      key: "geodisplay",
      value: JSON.stringify({
        type: "xz",
        code: "indexkey",
        maxLevel: 2,
        fullExtent: { xmin: 0, ymin: 0, xmax: 8, ymax: 8 },
      }),
    }];
    file.rowGroups[0] = {
      ...file.rowGroups[0],
      bounds: null,
      columns: [{
        columnIndex: 0,
        dataRange: { start: 0, end: 10 },
        columnIndexRange: null,
        offsetIndexRange: null,
        statistics: {
          numValues: 10,
          nullCount: 0,
          distinctCount: null,
          nanCount: null,
          min: { type: "int64", value: "2" },
          max: { type: "int64", value: "5" },
          minExact: true,
          maxExact: true,
        },
        compression: "UNCOMPRESSED",
        encodings: ["PLAIN"],
        compressedSize: 10,
        uncompressedSize: 10,
        dictionaryPageOffset: null,
        dataPageOffset: 0,
      }],
    };
    const snapshot: ParquetDiagnosticsSnapshot = { files: [file] };

    expect(deriveRowGroupBounds(snapshot, file)).toEqual([]);
  });
});

function createFile(): ParquetFileDiagnostics {
  return {
    version: 1,
    fileId: 10,
    fileName: "file.parquet",
    byteLength: 100,
    footerRange: { start: 90, end: 100 },
    keyValueMetadata: [],
    columns: [{
      index: 0,
      path: ["indexkey"],
      name: "indexkey",
      physicalType: "int64",
      logicalType: "uint64",
      nullable: false,
      maxDefinitionLevel: 0,
      maxRepetitionLevel: 0,
    }],
    rowGroups: [{
      index: 0,
      rowStart: 0,
      rowCount: 10,
      dataRange: { start: 0, end: 90 },
      bounds: {
        xmin: 0,
        ymin: 1,
        xmax: 2,
        ymax: 3,
        zmin: null,
        zmax: null,
        mmin: null,
        mmax: null,
      },
      columns: [],
    }],
  };
}

import type Extent from "@arcgis/core/geometry/Extent";
import type { QueryProperties } from "@arcgis/core/rest/support/Query";

import type {
  ParquetColumnChunkDiagnostics,
  ParquetDiagnosticsSnapshot,
  ParquetFileDiagnostics,
  ParquetNumericValue,
} from "../parquet/fileLayout";
import { extractGeodisplayMetadata } from "../parquet/keyValueMetadata";
import {
  getParquetObjectId,
  type ParquetFileId,
  type ParquetRowId,
} from "./parquetObjectId";

interface CustomExtentLayer {
  queryFeatures(query: QueryProperties): Promise<{
    features: Array<{
      geometry?: {
        extent?: Extent | null;
      } | null;
    }>;
  }>;
}

interface RowGroupCandidate {
  fileId: ParquetFileId;
  rowId: ParquetRowId;
  spread: bigint;
}

export async function inferCustomExtent(
  layer: CustomExtentLayer,
  snapshot: ParquetDiagnosticsSnapshot,
): Promise<Extent | null> {
  const candidate = findDensestRowGroup(snapshot);
  if (!candidate) {
    return null;
  }

  const objectId = getParquetObjectId(candidate.fileId, candidate.rowId);
  const result = await layer.queryFeatures({
    objectIds: [objectId],
    outFields: [],
    returnGeometry: true,
  });
  return result.features[0]?.geometry?.extent ?? null;
}

function findDensestRowGroup(
  snapshot: ParquetDiagnosticsSnapshot,
): RowGroupCandidate | null {
  let selectedCandidate: RowGroupCandidate | null = null;
  for (const file of snapshot.files) {
    const clusterKeyColumnIndex = findClusterKeyColumnIndex(file);
    if (clusterKeyColumnIndex === null) {
      continue;
    }

    for (const rowGroup of file.rowGroups) {
      const chunk = rowGroup.columns.find(
        ({ columnIndex }) => columnIndex === clusterKeyColumnIndex,
      );
      const spread = calculateXZCodeSpread(chunk);
      if (
        spread !== null &&
        (selectedCandidate === null || spread < selectedCandidate.spread)
      ) {
        selectedCandidate = {
          fileId: file.fileId,
          rowId: rowGroup.rowStart + Math.floor(rowGroup.rowCount / 2),
          spread,
        };
      }
    }
  }
  return selectedCandidate;
}

function findClusterKeyColumnIndex(file: ParquetFileDiagnostics): number | null {
  const metadata = extractGeodisplayMetadata(file.keyValueMetadata);
  if (!metadata || metadata.value.type !== "xz") {
    return null;
  }

  const declaredPath = readColumnPath(metadata.value.code);
  if (!declaredPath) {
    return null;
  }
  const clusterKeyPath = [...metadata.parentPath, ...declaredPath];
  return file.columns.find(({ path }) => pathsEqual(path, clusterKeyPath))
    ?.index ?? null;
}

function calculateXZCodeSpread(
  chunk: ParquetColumnChunkDiagnostics | undefined,
): bigint | null {
  const minimum = readXZCode(chunk?.statistics.min);
  const maximum = readXZCode(chunk?.statistics.max);
  return minimum !== null && maximum !== null && maximum >= minimum
    ? maximum - minimum
    : null;
}

function readXZCode(value: ParquetNumericValue | null | undefined): bigint | null {
  if (value?.type === "int32") {
    return Number.isInteger(value.value) ? BigInt(value.value) : null;
  }
  if (value?.type !== "int64") {
    return null;
  }

  try {
    return BigInt(value.value);
  } catch {
    return null;
  }
}

function readColumnPath(value: unknown): string[] | null {
  if (typeof value === "string" && value.length > 0) {
    return value.split(".");
  }
  return Array.isArray(value) &&
      value.length > 0 &&
      value.every((part) => typeof part === "string" && part.length > 0)
    ? value
    : null;
}

function pathsEqual(left: readonly string[], right: readonly string[]): boolean {
  return left.length === right.length &&
    left.every((part, index) => part === right[index]);
}

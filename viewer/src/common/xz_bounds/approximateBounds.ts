import type {
  ParquetColumnDiagnostics,
  ParquetNumericValue,
  ParquetDiagnosticsSnapshot,
  ParquetFileDiagnostics,
} from "../../parquet/fileLayout";
import {
  decodeXZBounds,
  type XZExtent,
} from "./decodeXZBounds";
import { extractGeodisplayMetadata } from "../../parquet/keyValueMetadata";
import type {
  ParquetPageIndexSource,
  ParquetPageStatistic,
} from "../../arcgis/file-explorer/inspector/parquetPageIndexes";

export interface ApproximateBound {
  fileId: number;
  fileName: string;
  rowGroupIndex: number;
  pageIndex: number | null;
  featureCount: number;
  approximate: boolean;
  xmin: number;
  ymin: number;
  xmax: number;
  ymax: number;
}

export interface BoundsExtent {
  xmin: number;
  ymin: number;
  xmax: number;
  ymax: number;
}

export async function approximateBounds(
  snapshot: ParquetDiagnosticsSnapshot,
  source: ParquetPageIndexSource,
): Promise<ApproximateBound[]> {
  const rowGroupCount = snapshot.files.reduce(
    (count, file) => count + file.rowGroups.length,
    0,
  );
  const rowGroupBounds = snapshot.files.flatMap((file) =>
    deriveRowGroupBounds(snapshot, file)
  );
  if (rowGroupCount >= 4) {
    return rowGroupBounds;
  }

  const pageBounds = await Promise.all(
    snapshot.files.map((file) => derivePageBounds(file, source)),
  );
  const resolvedPageBounds = pageBounds.flat();
  const datasetFeatureCount = snapshot.files.reduce(
    (fileCount, file) =>
      fileCount +
      file.rowGroups.reduce(
        (rowGroupCount, rowGroup) => rowGroupCount + rowGroup.rowCount,
        0,
      ),
    0,
  );
  const pageFeatureCount = resolvedPageBounds.reduce(
    (count, bound) => count + bound.featureCount,
    0,
  );
  return pageFeatureCount === datasetFeatureCount
    ? resolvedPageBounds
    : rowGroupBounds;
}

export function deriveRowGroupBounds(
  snapshot: ParquetDiagnosticsSnapshot,
  file: ParquetFileDiagnostics,
): ApproximateBound[] {
  if (!snapshot.files.includes(file)) {
    throw new Error("The selected diagnostics file does not belong to the snapshot.");
  }

  const xzMetadata = readXZMetadata(file);
  return file.rowGroups.flatMap((rowGroup) => {
    const bounds = rowGroup.bounds
      ? {
          approximate: false,
          xmin: rowGroup.bounds.xmin,
          ymin: rowGroup.bounds.ymin,
          xmax: rowGroup.bounds.xmax,
          ymax: rowGroup.bounds.ymax,
        }
      : deriveApproximateXZBounds(file.columns, rowGroup.columns, xzMetadata);
    if (!bounds) {
      return [];
    }

    return [{
      fileId: file.fileId,
      fileName: file.fileName,
      rowGroupIndex: rowGroup.index,
      pageIndex: null,
      featureCount: rowGroup.rowCount,
      ...bounds,
    }];
  });
}

interface XZMetadata {
  clusterKeyPath: string[];
  fullExtent: XZExtent;
  maxLevel: number;
}

function deriveApproximateXZBounds(
  columns: readonly ParquetColumnDiagnostics[],
  chunks: ParquetFileDiagnostics["rowGroups"][number]["columns"],
  metadata: XZMetadata | null,
): BoundsExtent & { approximate: true } | null {
  if (!metadata) {
    return null;
  }

  const column = columns.find(({ path }) =>
    pathsEqual(path, metadata.clusterKeyPath)
  );
  const chunk = column
    ? chunks.find(({ columnIndex }) => columnIndex === column.index)
    : undefined;
  const minimumCode = readXZCode(chunk?.statistics.min);
  const maximumCode = readXZCode(chunk?.statistics.max);
  if (minimumCode === null || maximumCode === null) {
    return null;
  }

  try {
    return {
      approximate: true,
      ...decodeXZBounds(
        { min: minimumCode, max: maximumCode },
        metadata.fullExtent,
        metadata.maxLevel,
      ),
    };
  } catch {
    return null;
  }
}

async function derivePageBounds(
  file: ParquetFileDiagnostics,
  source: ParquetPageIndexSource,
): Promise<ApproximateBound[]> {
  const metadata = readXZMetadata(file);
  if (!metadata) {
    return [];
  }
  const column = file.columns.find(({ path }) =>
    pathsEqual(path, metadata.clusterKeyPath)
  );
  if (!column) {
    return [];
  }

  const bounds = await Promise.all(file.rowGroups.map(async (rowGroup) => {
    const target = {
      fileId: file.fileId,
      rowGroupIndex: rowGroup.index,
      columnIndex: column.index,
    };
    const [columnIndex, offsetIndex] = await Promise.all([
      source.getColumnIndex(target),
      source.getOffsetIndex(target),
    ]);
    if (
      !columnIndex ||
      !offsetIndex ||
      columnIndex.pages.length !== offsetIndex.pages.length
    ) {
      return [];
    }

    return columnIndex.pages.flatMap((statistic, pageIndex) => {
      const page = offsetIndex.pages[pageIndex];
      const extent = derivePageExtent(statistic, metadata);
      if (!page || !extent || page.rowEnd <= page.rowStart) {
        return [];
      }
      return [{
        fileId: file.fileId,
        fileName: file.fileName,
        rowGroupIndex: rowGroup.index,
        pageIndex: page.pageIndex,
        featureCount: page.rowEnd - page.rowStart,
        approximate: true,
        ...extent,
      }];
    });
  }));
  return bounds.flat();
}

function derivePageExtent(
  statistic: ParquetPageStatistic,
  metadata: XZMetadata,
): BoundsExtent | null {
  if (statistic.type !== "bounds") {
    return null;
  }
  const minimumCode = readXZCode(statistic.min);
  const maximumCode = readXZCode(statistic.max);
  if (minimumCode === null || maximumCode === null) {
    return null;
  }
  try {
    return decodeXZBounds(
      { min: minimumCode, max: maximumCode },
      metadata.fullExtent,
      metadata.maxLevel,
    );
  } catch {
    return null;
  }
}

function readXZMetadata(file: ParquetFileDiagnostics): XZMetadata | null {
  const metadata = extractGeodisplayMetadata(file.keyValueMetadata);
  if (!metadata) {
    return null;
  }

  const value = metadata.value;
  if (value.type !== "xz") {
    return null;
  }
  const declaredClusterKeyPath = readColumnPath(value.code);
  const fullExtent = readExtent(value.fullExtent);
  const maxLevel = value.maxLevel;
  return declaredClusterKeyPath && fullExtent && Number.isInteger(maxLevel)
    ? {
        clusterKeyPath: [
          ...metadata.parentPath,
          ...declaredClusterKeyPath,
        ],
        fullExtent,
        maxLevel: maxLevel as number,
      }
    : null;
}

function readXZCode(value: ParquetNumericValue | null | undefined): string | number | null {
  return value?.type === "int32" || value?.type === "int64"
    ? value.value
    : null;
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

function readExtent(value: unknown): XZExtent | null {
  if (!isRecord(value)) {
    return null;
  }
  const extent = {
    xmin: value.xmin,
    ymin: value.ymin,
    xmax: value.xmax,
    ymax: value.ymax,
  };
  return Object.values(extent).every(
    (coordinate) => typeof coordinate === "number" && Number.isFinite(coordinate),
  )
    ? extent as XZExtent
    : null;
}

function pathsEqual(left: readonly string[], right: readonly string[]): boolean {
  return left.length === right.length &&
    left.every((part, index) => part === right[index]);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

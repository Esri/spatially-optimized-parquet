import type { ParquetByteCoverage } from "../../../parquet/ParquetByteCoverage";
import type { DownloadTrackLayout } from "../../../parquet/displayLayout";
import type {
  ColumnStatisticValue,
  FileLayout,
} from "../../../parquet/fileLayout";
import type { RowGroupBound } from "../../../common/rowGroupBounds";

export interface DownloadTopologySnapshot {
  layout: FileLayout | null;
  tracks: readonly DownloadTrackLayout[];
  rowGroupBounds: readonly RowGroupBound[] | null;
  error: Error | null;
}

export interface DownloadBlockSnapshot {
  cachedMask: number;
  loadingMask: number;
  activeMask: number;
}

export interface DownloadTrackSnapshot {
  downloadedByteLength: number;
  filledSubpartCount: number;
  subpartCount: number;
  visibleBlockCount: number;
  visibleBlockIds: readonly string[];
}

export interface DownloadSummarySnapshot {
  downloadedByteLength: number;
  completedRequestCount: number;
  cachedSubpartCount: number;
  subpartCount: number;
  visibleColumnCount: number;
  columnCount: number;
}

export interface DownloadRowGroupCoverage {
  rowGroupIndex: number;
  fileId?: number;
  fileName?: string;
  sourceRowGroupIndex?: number;
  downloadedByteLength: number;
  byteLength: number;
  downloadedPercent: number;
  statistics: DownloadColumnStatistics | null;
  itemCoverage: readonly DownloadRowGroupItemCoverage[];
}

export interface DownloadColumnStatistics {
  minimumValue: ColumnStatisticValue | null;
  maximumValue: ColumnStatisticValue | null;
  nullCount: number | null;
  recordCount: number | null;
}

export interface DownloadRowGroupItemCoverage extends DownloadColumnStatistics {
  label: string;
  downloadedByteLength: number;
  byteLength: number;
  downloaded: boolean;
}

export interface ReadonlyExternalStore<Snapshot> {
  subscribe(listener: () => void): () => void;
  getSnapshot(): Snapshot;
}

export interface DownloadSessionView {
  readonly topology: ReadonlyExternalStore<DownloadTopologySnapshot>;
  readonly summary: ReadonlyExternalStore<DownloadSummarySnapshot>;
  block(blockId: string): ReadonlyExternalStore<DownloadBlockSnapshot>;
  track(trackId: string): ReadonlyExternalStore<DownloadTrackSnapshot>;
  rowGroupCoverage(trackId: string): readonly DownloadRowGroupCoverage[];
  rowGroupBlockMasks(
    trackId: string,
    rowGroupIndex: number,
  ): ReadonlyMap<string, number>;
  createFileStructureSnapshot(): {
    layout: FileLayout;
    coverage: ParquetByteCoverage;
  } | null;
}

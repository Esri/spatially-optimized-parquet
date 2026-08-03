import type {
  ParquetDiagnosticsSnapshot,
  ParquetFileDiagnostics,
  ArcgisParquetRangeReadEvent,
} from "../../diagnostics";
import type { ParquetByteCoverage } from "../../../parquet/ParquetByteCoverage";
import {
  createDownloadDisplayLayout,
  createDatasetDownloadDisplayLayout,
  type DownloadDisplayLayout,
} from "../../../parquet/displayLayout";
import {
  type FileLayout,
  deriveFileLayout,
} from "../../../parquet/fileLayout";
import {
  deriveRowGroupBounds,
  type RowGroupBound,
} from "../../../common/rowGroupBounds";
import { ParquetDownloadCoverage } from "./ParquetDownloadCoverage";
import {
  ParquetDownloadProjection,
  type ProjectionChange,
} from "./ParquetDownloadProjection";
import { DownloadSessionPublisher } from "./DownloadSessionPublisher";
import { RangeReadLedger } from "./RangeReadLedger";
import type {
  DownloadBlockSnapshot,
  DownloadRowGroupCoverage,
  DownloadSessionView,
  DownloadSummarySnapshot,
  DownloadTopologySnapshot,
  DownloadTrackSnapshot,
  ReadonlyExternalStore,
} from "./types";

export type {
  DownloadBlockSnapshot,
  DownloadColumnStatistics,
  DownloadRowGroupCoverage,
  DownloadRowGroupItemCoverage,
  DownloadSessionView,
  DownloadSummarySnapshot,
  DownloadTopologySnapshot,
  DownloadTrackSnapshot,
  ReadonlyExternalStore,
} from "./types";

const emptyBlockSnapshot: DownloadBlockSnapshot = {
  cachedMask: 0,
  loadingMask: 0,
  activeMask: 0,
};

const emptyTrackSnapshot: DownloadTrackSnapshot = {
  downloadedByteLength: 0,
  filledSubpartCount: 0,
  subpartCount: 0,
  visibleBlockCount: 0,
  visibleBlockIds: [],
};


/**
 * Coordinates diagnostics, byte coverage, request activity, and published download snapshots.
 * It owns the session lifecycle so file-explorer views consume one consistent model instead of synchronizing those concerns themselves.
 */
export class ParquetDownloadSession implements DownloadSessionView {
  private _layout: FileLayout | null = null;
  private _rowGroupBounds: RowGroupBound[] | null = null;
  private _error: Error | null = null;
  private _acceptedFileNames = new Set<string>();
  private _fileOffsets = new Map<string, number>();
  private readonly _coverage = new ParquetDownloadCoverage();
  private readonly _ledger = new RangeReadLedger();
  private readonly _projection = new ParquetDownloadProjection();
  private readonly _publisher: DownloadSessionPublisher;

  readonly topology: ReadonlyExternalStore<DownloadTopologySnapshot>;
  readonly summary: ReadonlyExternalStore<DownloadSummarySnapshot>;

  constructor() {
    this._publisher = new DownloadSessionPublisher(
      {
        createTopologySnapshot: () => this._projection.createTopologySnapshot({
          layout: this._layout,
          rowGroupBounds: this._rowGroupBounds,
          error: this._error,
        }),
        createSummarySnapshot: () => this._projection.createSummarySnapshot(
          this._coverage.downloadedByteLength,
          this._ledger.completedRequestCount,
        ),
        createBlockSnapshot: (blockId) => this._projection.createBlockSnapshot(
          blockId,
          this._ledger.latestRequestKey,
        ),
        createTrackSnapshot: (trackId) => this._projection.createTrackSnapshot(trackId),
      },
      emptyBlockSnapshot,
      emptyTrackSnapshot,
    );
    this.topology = this._publisher.topology;
    this.summary = this._publisher.summary;
  }

  reset(): void {
    this._publisher.resetSnapshots();
    this._layout = null;
    this._rowGroupBounds = null;
    this._error = null;
    this._acceptedFileNames.clear();
    this._fileOffsets.clear();
    this._coverage.reset();
    this._ledger.reset();
    this._projection.reset();
    this._publisher.markSummaryDirty();
    this._publisher.markTopologyDirty();
    this._publisher.flush(true);
  }

  loadDiagnostics(
    diagnostics: ParquetDiagnosticsSnapshot,
    file: ParquetFileDiagnostics,
  ): void {
    try {
      const layout = deriveFileLayout(diagnostics, file);
      const displayLayout = createDownloadDisplayLayout(layout);
      this._initializeLayout({
        acceptedFileNames: [file.fileName],
        displayLayout,
        fileOffsets: new Map([[file.fileName, 0]]),
        layout,
        rowGroupBounds: deriveRowGroupBounds(diagnostics, file),
      });
    } catch (cause) {
      const error = cause instanceof Error
        ? cause
        : new Error("Failed to derive Parquet diagnostics layout.");
      this.reportError(error);
      throw error;
    }
  }

  loadDatasetLayouts(
    layouts: readonly FileLayout[],
    rowGroupBounds: readonly RowGroupBound[],
  ): void {
    const datasetLayout = createDatasetDownloadDisplayLayout(layouts);
    const layout: FileLayout = {
      fileId: -1,
      fileName: "All files",
      byteLength: datasetLayout.byteLength,
      footer: {
        start: datasetLayout.byteLength,
        end: datasetLayout.byteLength,
      },
      keyValueMetadata: [],
      rowGroups: [],
      pageIndexes: [],
    };
    this._initializeLayout({
      acceptedFileNames: layouts.map(({ fileName }) => fileName),
      displayLayout: datasetLayout.displayLayout,
      fileOffsets: datasetLayout.fileOffsets,
      layout,
      rowGroupBounds,
    });
  }

  recordRangeRead(event: ArcgisParquetRangeReadEvent): void {
    const fileOffset = this._fileOffsets.get(event.fileId);
    const normalizedEvent = fileOffset === undefined
      ? event
      : {
          ...event,
          range: {
            start: event.range.start + fileOffset,
            end: event.range.end + fileOffset,
          },
        };
    const change = this._ledger.record(
      normalizedEvent,
      (fileId) => this._acceptedFileNames.has(fileId),
    );
    if (change.type === "duplicate") {
      return;
    }
    if (change.type === "unknown-file") {
      this.reportError(
        new Error(`Range event references unknown diagnostics file "${change.fileId}".`),
      );
      return;
    }

    if (change.type === "start") {
      this._publisher.markBlocks(
        this._projection.requestImpactBlockIds(change.previousLatestRequestKey),
      );
      this._publishChange(
        this._projection.applyLoadingRequestImpact(
          change.requestKey,
          change.range,
          1,
        ),
      );
      this._publisher.markBlocks(
        this._projection.requestImpactBlockIds(change.requestKey),
      );
      this._publisher.schedule();
      return;
    }

    this._publishChange(
      this._projection.applyLoadingRequestImpact(change.requestKey, undefined, -1),
    );
    if (change.replacedLatestRequest) {
      this._publisher.markBlocks(
        this._projection.requestImpactBlockIds(change.latestRequestKey),
      );
    }
    if (change.phase === "complete") {
      const { newRanges } = this._coverage.add(change.range);
      this._publishChange(this._projection.addCoveredRanges(newRanges));
      this._publishChange(
        this._projection.applyCachedRequestImpact(change.requestKey, change.range),
      );
      this._publisher.markSummaryDirty();
    }
    this._publisher.schedule();
  }

  reportError(error: Error): void {
    this._error = error;
    this._publisher.markTopologyDirty();
    this._publisher.schedule();
  }

  dispose(): void {
    this.reset();
  }

  private _initializeLayout({
    acceptedFileNames,
    displayLayout,
    fileOffsets,
    layout,
    rowGroupBounds,
  }: {
    acceptedFileNames: readonly string[];
    displayLayout: DownloadDisplayLayout;
    fileOffsets: ReadonlyMap<string, number>;
    layout: FileLayout;
    rowGroupBounds: readonly RowGroupBound[];
  }): void {
    this._acceptedFileNames = new Set(acceptedFileNames);
    this._fileOffsets = new Map(fileOffsets);
    this._layout = layout;
    this._rowGroupBounds = [...rowGroupBounds];
    this._error = null;
    this._publishChange(this._projection.initialize(displayLayout));
    this._replayCoverage();
    this._publisher.markTopologyDirty();
    this._publisher.markSummaryDirty();
    this._publisher.flush(true);
  }

  block(blockId: string): ReadonlyExternalStore<DownloadBlockSnapshot> {
    return this._publisher.block(blockId);
  }

  track(trackId: string): ReadonlyExternalStore<DownloadTrackSnapshot> {
    return this._publisher.track(trackId);
  }

  rowGroupCoverage(trackId: string): readonly DownloadRowGroupCoverage[] {
    return this._projection.rowGroupCoverage(trackId);
  }

  rowGroupBlockMasks(
    trackId: string,
    rowGroupIndex: number,
  ): ReadonlyMap<string, number> {
    return this._projection.rowGroupBlockMasks(
      trackId,
      rowGroupIndex,
      (range) => this._coverage.overlaps(range),
    );
  }

  createFileStructureSnapshot(): {
    layout: FileLayout;
    coverage: ParquetByteCoverage;
  } | null {
    return this._coverage.createFileStructureSnapshot(this._layout);
  }

  private _replayCoverage(): void {
    for (const range of this._coverage.values) {
      this._publishChange(this._projection.applyCachedRange(range));
    }
    this._publishChange(this._projection.addCoveredRanges(this._coverage.values));
    for (const [requestKey, request] of this._ledger.activeRequestEntries) {
      this._publishChange(
        this._projection.applyLoadingRequestImpact(requestKey, request.range, 1),
      );
    }
  }

  private _publishChange(change: ProjectionChange): void {
    this._publisher.markBlocks(change.blockIds);
    this._publisher.markTracks(change.trackIds);
  }
}
import type {
  ArcgisParquetDiagnosticsSnapshotV1,
  ArcgisParquetRangeReadEvent,
} from "../../diagnostics";
import type { ParquetByteCoverage } from "../../../../../parquet/byteCoverage";
import {
  createDownloadDisplayLayout,
  type DownloadDisplayLayout,
} from "../../../../../parquet/displayLayout";
import {
  type FileLayout,
  deriveFileLayout,
} from "../../../../../parquet/fileLayout";
import {
  deriveRowGroupBounds,
  type RowGroupBounds,
} from "../../../../../parquet/rowGroupBounds";
import { ParquetDownloadCoverage } from "./coverage";
import {
  ParquetDownloadProjection,
  type ProjectionChange,
} from "./projection";
import { DownloadSessionPublisher } from "./publisher";
import { RangeReadLedger } from "./ledger";
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

export class ParquetDownloadSession implements DownloadSessionView {
  private diagnostics: ArcgisParquetDiagnosticsSnapshotV1 | null = null;
  private layout: FileLayout | null = null;
  private displayLayout: DownloadDisplayLayout | null = null;
  private rowGroupBounds: RowGroupBounds[] | null = null;
  private error: Error | null = null;
  private readonly coverage = new ParquetDownloadCoverage();
  private readonly ledger = new RangeReadLedger();
  private readonly projection = new ParquetDownloadProjection();
  private readonly publisher: DownloadSessionPublisher;

  readonly topology: ReadonlyExternalStore<DownloadTopologySnapshot>;
  readonly summary: ReadonlyExternalStore<DownloadSummarySnapshot>;

  constructor() {
    this.publisher = new DownloadSessionPublisher(
      {
        createTopologySnapshot: () => this.projection.createTopologySnapshot({
          layout: this.layout,
          rowGroupBounds: this.rowGroupBounds,
          error: this.error,
        }),
        createSummarySnapshot: () => this.projection.createSummarySnapshot(
          this.coverage.currentDownloadedByteLength(),
          this.ledger.currentCompletedRequestCount(),
        ),
        createBlockSnapshot: (blockId) => this.projection.createBlockSnapshot(
          blockId,
          this.ledger.currentLatestRequestKey(),
        ),
        createTrackSnapshot: (trackId) => this.projection.createTrackSnapshot(trackId),
      },
      emptyBlockSnapshot,
      emptyTrackSnapshot,
    );
    this.topology = this.publisher.topology;
    this.summary = this.publisher.summary;
  }

  reset(): void {
    this.publisher.resetSnapshots();
    this.diagnostics = null;
    this.layout = null;
    this.displayLayout = null;
    this.rowGroupBounds = null;
    this.error = null;
    this.coverage.reset();
    this.ledger.reset();
    this.projection.reset();
    this.publisher.markSummaryDirty();
    this.publisher.markTopologyDirty();
    this.publisher.flush(true);
  }

  loadDiagnostics(diagnostics: ArcgisParquetDiagnosticsSnapshotV1): void {
    try {
      const layout = deriveFileLayout(diagnostics);
      const displayLayout = createDownloadDisplayLayout(layout);
      this.diagnostics = diagnostics;
      this.layout = layout;
      this.displayLayout = displayLayout;
      this.rowGroupBounds = deriveRowGroupBounds(diagnostics);
      this.error = null;
      this.publishChange(this.projection.initialize(displayLayout));
      this.replayCoverage();
      this.publisher.markTopologyDirty();
      this.publisher.markSummaryDirty();
      this.publisher.flush(true);
    } catch (cause) {
      const error = cause instanceof Error
        ? cause
        : new Error("Failed to derive Parquet diagnostics layout.");
      this.reportError(error);
      throw error;
    }
  }

  recordRangeRead(event: ArcgisParquetRangeReadEvent): void {
    const change = this.ledger.record(
      event,
      (fileId) =>
        !this.diagnostics ||
        this.diagnostics.files.some((file) => file.fileName === fileId),
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
      this.publisher.markBlocks(
        this.projection.requestImpactBlockIds(change.previousLatestRequestKey),
      );
      this.publishChange(
        this.projection.applyLoadingRequestImpact(
          change.requestKey,
          change.range,
          1,
        ),
      );
      this.publisher.markBlocks(
        this.projection.requestImpactBlockIds(change.requestKey),
      );
      this.publisher.schedule();
      return;
    }

    this.publishChange(
      this.projection.applyLoadingRequestImpact(change.requestKey, undefined, -1),
    );
    if (change.replacedLatestRequest) {
      this.publisher.markBlocks(
        this.projection.requestImpactBlockIds(change.latestRequestKey),
      );
    }
    if (change.phase === "complete") {
      const { newRanges } = this.coverage.add(change.range);
      this.publishChange(this.projection.addCoveredRanges(newRanges));
      this.publishChange(
        this.projection.applyCachedRequestImpact(change.requestKey, change.range),
      );
      this.publisher.markSummaryDirty();
    }
    this.publisher.schedule();
  }

  reportError(error: Error): void {
    this.error = error;
    this.publisher.markTopologyDirty();
    this.publisher.schedule();
  }

  block(blockId: string): ReadonlyExternalStore<DownloadBlockSnapshot> {
    return this.publisher.block(blockId);
  }

  track(trackId: string): ReadonlyExternalStore<DownloadTrackSnapshot> {
    return this.publisher.track(trackId);
  }

  rowGroupCoverage(trackId: string): readonly DownloadRowGroupCoverage[] {
    return this.projection.rowGroupCoverage(trackId);
  }

  rowGroupBlockMasks(
    trackId: string,
    rowGroupIndex: number,
  ): ReadonlyMap<string, number> {
    return this.projection.rowGroupBlockMasks(
      trackId,
      rowGroupIndex,
      (range) => this.coverage.overlaps(range),
    );
  }

  createFileStructureSnapshot(): {
    layout: FileLayout;
    coverage: ParquetByteCoverage;
  } | null {
    return this.coverage.createFileStructureSnapshot(this.layout);
  }

  private replayCoverage(): void {
    for (const range of this.coverage.values()) {
      this.publishChange(this.projection.applyCachedRange(range));
    }
    this.publishChange(this.projection.addCoveredRanges(this.coverage.values()));
    for (const [requestKey, request] of this.ledger.activeRequestEntries()) {
      this.publishChange(
        this.projection.applyLoadingRequestImpact(requestKey, request.range, 1),
      );
    }
  }

  private publishChange(change: ProjectionChange): void {
    this.publisher.markBlocks(change.blockIds);
    this.publisher.markTracks(change.trackIds);
  }
}

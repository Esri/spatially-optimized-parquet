import type {
  ArcgisParquetDiagnosticsSnapshotV1,
  ArcgisParquetRangeReadEvent,
} from "./arcgisParquetDiagnostics";
import { ParquetByteCoverage } from "./parquetByteCoverage";
import { ByteRangeIndex } from "./byteRangeIndex";
import {
  createDownloadDisplayLayout,
  type DownloadBlockLayout,
  type DownloadDisplayLayout,
  type DownloadPhysicalSegment,
  type DownloadTrackLayout,
  displaySubpartCount,
} from "./parquetDisplayLayout";
import {
  type ByteRange,
  type ColumnStatisticValue,
  type FileLayout,
  deriveFileLayout,
  rangesOverlap,
} from "./parquetFileLayout";
import {
  deriveRowGroupBounds,
  type RowGroupBounds,
} from "./parquetRowGroupBounds";

const updateIntervalMs = 64;
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

export interface DownloadTopologySnapshot {
  layout: FileLayout | null;
  tracks: readonly DownloadTrackLayout[];
  rowGroupBounds: readonly RowGroupBounds[] | null;
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

interface ActiveRequest {
  range: ByteRange;
}

interface MutableBlockState {
  cachedMask: number;
  loadingCount: number[];
}

interface MutableTrackState {
  downloadedByteLength: number;
  cachedSubpartCount: number;
  visibleBlockCount: number;
  visibleBlockIds: string[];
}

export class ParquetDownloadStore {
  private diagnostics: ArcgisParquetDiagnosticsSnapshotV1 | null = null;
  private layout: FileLayout | null = null;
  private displayLayout: DownloadDisplayLayout | null = null;
  private rowGroupBounds: RowGroupBounds[] | null = null;
  private coverage = new ParquetByteCoverage();
  private activeRequests = new Map<string, ActiveRequest>();
  private requestImpacts = new Map<string, Map<string, number>>();
  private handledEventKeys = new Set<string>();
  private latestRequestKey: string | null = null;
  private error: Error | null = null;
  private downloadedByteLength = 0;
  private completedRequestCount = 0;

  private physicalIndex = new ByteRangeIndex<DownloadPhysicalSegment>([]);
  private readonly blockById = new Map<string, DownloadBlockLayout>();
  private readonly blockOrderById = new Map<string, number>();
  private readonly trackById = new Map<string, DownloadTrackLayout>();
  private readonly blockState = new Map<string, MutableBlockState>();
  private readonly trackState = new Map<string, MutableTrackState>();
  private readonly segmentCoveredByteLength = new Map<string, number>();
  private readonly blockSnapshots = new Map<string, DownloadBlockSnapshot>();
  private readonly trackSnapshots = new Map<string, DownloadTrackSnapshot>();
  private summarySnapshot: DownloadSummarySnapshot = {
    downloadedByteLength: 0,
    completedRequestCount: 0,
    cachedSubpartCount: 0,
    subpartCount: 0,
    visibleColumnCount: 0,
    columnCount: 0,
  };
  private topologySnapshot: DownloadTopologySnapshot = this.createTopologySnapshot();

  private readonly topologyListeners = new Set<() => void>();
  private readonly summaryListeners = new Set<() => void>();
  private readonly blockListeners = new Map<string, Set<() => void>>();
  private readonly trackListeners = new Map<string, Set<() => void>>();
  private readonly dirtyBlockIds = new Set<string>();
  private readonly dirtyTrackIds = new Set<string>();
  private summaryDirty = false;
  private topologyDirty = false;
  private flushTimer: ReturnType<typeof setTimeout> | null = null;
  private lastFlushTime = Number.NEGATIVE_INFINITY;

  subscribeTopology = (listener: () => void): (() => void) => {
    this.topologyListeners.add(listener);
    return () => this.topologyListeners.delete(listener);
  };

  getTopologySnapshot = (): DownloadTopologySnapshot => this.topologySnapshot;

  subscribeSummary = (listener: () => void): (() => void) => {
    this.summaryListeners.add(listener);
    return () => this.summaryListeners.delete(listener);
  };

  getSummarySnapshot = (): DownloadSummarySnapshot => this.summarySnapshot;

  subscribeBlock(blockId: string, listener: () => void): () => void {
    const listeners = this.blockListeners.get(blockId) ?? new Set<() => void>();
    listeners.add(listener);
    this.blockListeners.set(blockId, listeners);
    return () => {
      listeners.delete(listener);
      if (listeners.size === 0) {
        this.blockListeners.delete(blockId);
      }
    };
  }

  getBlockSnapshot(blockId: string): DownloadBlockSnapshot {
    return this.blockSnapshots.get(blockId) ?? emptyBlockSnapshot;
  }

  subscribeTrack(trackId: string, listener: () => void): () => void {
    const listeners = this.trackListeners.get(trackId) ?? new Set<() => void>();
    listeners.add(listener);
    this.trackListeners.set(trackId, listeners);
    return () => {
      listeners.delete(listener);
      if (listeners.size === 0) {
        this.trackListeners.delete(trackId);
      }
    };
  }

  getTrackSnapshot(trackId: string): DownloadTrackSnapshot {
    return this.trackSnapshots.get(trackId) ?? emptyTrackSnapshot;
  }

  getSnapshot = (): DownloadTopologySnapshot => this.topologySnapshot;

  reset(): void {
    this.cancelFlush();
    this.diagnostics = null;
    this.layout = null;
    this.displayLayout = null;
    this.rowGroupBounds = null;
    this.coverage = new ParquetByteCoverage();
    this.activeRequests.clear();
    this.requestImpacts.clear();
    this.handledEventKeys.clear();
    this.latestRequestKey = null;
    this.error = null;
    this.downloadedByteLength = 0;
    this.completedRequestCount = 0;
    this.physicalIndex = new ByteRangeIndex([]);
    this.blockById.clear();
    this.blockOrderById.clear();
    this.trackById.clear();
    this.blockState.clear();
    this.trackState.clear();
    this.segmentCoveredByteLength.clear();
    this.blockSnapshots.clear();
    this.trackSnapshots.clear();
    this.requestImpacts.clear();
    this.dirtyBlockIds.clear();
    this.dirtyTrackIds.clear();
    this.summaryDirty = true;
    this.topologyDirty = true;
    this.flushDirtySnapshots(true);
  }

  setDiagnostics(diagnostics: ArcgisParquetDiagnosticsSnapshotV1): void {
    try {
      const layout = deriveFileLayout(diagnostics);
      const displayLayout = createDownloadDisplayLayout(layout);
      this.diagnostics = diagnostics;
      this.layout = layout;
      this.displayLayout = displayLayout;
      this.rowGroupBounds = deriveRowGroupBounds(diagnostics);
      this.error = null;
      this.initializeDisplayLayout(displayLayout);
      this.replayCoverage();
      this.topologyDirty = true;
      this.markSummaryDirty();
      this.flushDirtySnapshots(true);
    } catch (cause) {
      const error = cause instanceof Error
        ? cause
        : new Error("Failed to derive Parquet diagnostics layout.");
      this.setError(error);
      throw error;
    }
  }

  handleRangeRead(event: ArcgisParquetRangeReadEvent): void {
    const eventKey = `${createRequestKey(event)}:${event.phase}`;
    if (this.handledEventKeys.has(eventKey)) {
      return;
    }
    if (this.diagnostics && !this.diagnostics.files.some((file) => file.fileName === event.fileId)) {
      this.setError(new Error(`Range event references unknown diagnostics file "${event.fileId}".`));
      return;
    }

    this.handledEventKeys.add(eventKey);
    const requestKey = createRequestKey(event);
    if (event.phase === "start") {
      this.markRequestImpactDirty(this.latestRequestKey);
      this.latestRequestKey = requestKey;
      this.activeRequests.set(requestKey, { range: event.range });
      this.applyLoadingImpact(requestKey, 1);
      this.markRequestImpactDirty(requestKey);
      this.scheduleFlush();
      return;
    }

    this.applyLoadingImpact(requestKey, -1);
    this.activeRequests.delete(requestKey);
    if (this.latestRequestKey === requestKey) {
      this.latestRequestKey = this.activeRequests.keys().next().value ?? null;
      this.markRequestImpactDirty(this.latestRequestKey);
    }

    if (event.phase === "complete") {
      this.completedRequestCount += 1;
      const { addedByteLength, newRanges: newlyCoveredRanges } =
        this.coverage.add(event.range);
      this.downloadedByteLength += addedByteLength;
      this.addTrackCoverage(newlyCoveredRanges);
      this.applyCachedImpact(requestKey, event.range);
      this.markSummaryDirty();
    }
    this.scheduleFlush();
  }

  setError(error: Error): void {
    this.error = error;
    this.topologyDirty = true;
    this.scheduleFlush();
  }

  private initializeDisplayLayout(displayLayout: DownloadDisplayLayout): void {
    this.physicalIndex = new ByteRangeIndex(
      displayLayout.segments.map((segment) => ({ range: segment.physicalRange, value: segment })),
    );
    this.blockById.clear();
    this.blockOrderById.clear();
    this.trackById.clear();
    this.blockState.clear();
    this.trackState.clear();
    this.segmentCoveredByteLength.clear();
    this.blockSnapshots.clear();
    this.trackSnapshots.clear();
    for (const track of displayLayout.tracks) {
      this.trackById.set(track.id, track);
      this.trackState.set(track.id, {
        downloadedByteLength: 0,
        cachedSubpartCount: 0,
        visibleBlockCount: 0,
        visibleBlockIds: [],
      });
      this.dirtyTrackIds.add(track.id);
      for (const [blockIndex, block] of track.blocks.entries()) {
        this.blockById.set(block.id, block);
        this.blockOrderById.set(block.id, blockIndex);
        this.blockState.set(block.id, {
          cachedMask: 0,
          loadingCount: Array<number>(displaySubpartCount).fill(0),
        });
      }
      for (const segment of track.segments) {
        this.segmentCoveredByteLength.set(segment.segmentId, 0);
      }
    }
  }

  getTrackRowGroupCoverage(trackId: string): readonly DownloadRowGroupCoverage[] {
    const track = this.trackById.get(trackId);
    if (!track) {
      return [];
    }

    const coverageByRowGroup = new Map<number, {
      rowGroupIndex: number;
      downloadedByteLength: number;
      byteLength: number;
      statistics: DownloadColumnStatistics | null;
      itemCoverageByLabel: Map<string, Omit<DownloadRowGroupItemCoverage, "downloaded">>;
    }>();
    for (const segment of track.segments) {
      if (segment.rowGroupIndex === null) {
        continue;
      }
      const byteLength = segment.physicalRange.end - segment.physicalRange.start;
      const coveredByteLength = this.segmentCoveredByteLength.get(segment.segmentId) ?? 0;
      const rowGroupCoverage = coverageByRowGroup.get(segment.rowGroupIndex) ?? {
        rowGroupIndex: segment.rowGroupIndex,
        downloadedByteLength: 0,
        byteLength: 0,
        statistics: track.kind === "column"
          ? {
              minimumValue: segment.minimumValue,
              maximumValue: segment.maximumValue,
              nullCount: segment.nullCount,
              recordCount: segment.recordCount,
            }
          : null,
        itemCoverageByLabel: new Map(),
      };
      rowGroupCoverage.downloadedByteLength += coveredByteLength;
      rowGroupCoverage.byteLength += byteLength;
      if (segment.detailLabel !== null) {
        const itemCoverage = rowGroupCoverage.itemCoverageByLabel.get(segment.detailLabel) ?? {
          label: segment.detailLabel,
          downloadedByteLength: 0,
          byteLength: 0,
          minimumValue: segment.minimumValue,
          maximumValue: segment.maximumValue,
          nullCount: segment.nullCount,
          recordCount: segment.recordCount,
        };
        itemCoverage.downloadedByteLength += coveredByteLength;
        itemCoverage.byteLength += byteLength;
        rowGroupCoverage.itemCoverageByLabel.set(segment.detailLabel, itemCoverage);
      }
      coverageByRowGroup.set(segment.rowGroupIndex, rowGroupCoverage);
    }

    return Array.from(coverageByRowGroup.values(), (rowGroupCoverage) => {
      const {
        itemCoverageByLabel,
        ...coverage
      } = rowGroupCoverage;
      return {
        ...coverage,
        downloadedPercent: coverage.byteLength === 0
          ? 0
          : Math.min(
              100,
              (coverage.downloadedByteLength / coverage.byteLength) * 100,
            ),
        itemCoverage: Array.from(itemCoverageByLabel.values(), (itemCoverage) => ({
          ...itemCoverage,
          downloaded:
            itemCoverage.byteLength > 0 &&
            itemCoverage.downloadedByteLength >= itemCoverage.byteLength,
        })),
      };
    });
  }

  getTrackRowGroupBlockMasks(
    trackId: string,
    rowGroupIndex: number,
  ): ReadonlyMap<string, number> {
    const track = this.trackById.get(trackId);
    if (!track) {
      return new Map();
    }
    const segmentIds = new Set(
      track.segments
        .filter((segment) => segment.rowGroupIndex === rowGroupIndex)
        .map((segment) => segment.segmentId),
    );
    const blockMasks = new Map<string, number>();

    for (const block of track.blocks) {
      let mask = 0;
      for (const subpart of block.subparts) {
        const hasDownloadedPiece = subpart.physicalPieces.some(
          (piece) =>
            segmentIds.has(piece.segmentId) &&
            this.coverage.overlaps(piece.physicalRange),
        );
        if (hasDownloadedPiece) {
          mask |= 1 << subpart.index;
        }
      }
      if (mask !== 0) {
        blockMasks.set(block.id, mask);
      }
    }

    return blockMasks;
  }

  private replayCoverage(): void {
    for (const range of this.coverage.values()) {
      for (const [blockId, mask] of this.resolveRangeImpact(range)) {
        this.updateBlockState(blockId, (state) => {
          state.cachedMask |= mask;
        });
      }
    }
    this.addTrackCoverage(this.coverage.values());
    for (const requestKey of this.activeRequests.keys()) {
      this.applyLoadingImpact(requestKey, 1);
    }
  }

  private applyLoadingImpact(requestKey: string, direction: 1 | -1): void {
    const request = this.activeRequests.get(requestKey);
    if (!request && direction > 0) {
      return;
    }
    const impact = this.getRequestImpact(requestKey, request?.range);
    for (const [blockId, mask] of impact) {
      this.updateBlockState(blockId, (state) => {
        for (let index = 0; index < displaySubpartCount; index += 1) {
          if ((mask & (1 << index)) !== 0) {
            state.loadingCount[index] = Math.max(0, state.loadingCount[index] + direction);
          }
        }
      });
    }
  }

  private applyCachedImpact(requestKey: string, range?: ByteRange): void {
    for (const [blockId, mask] of this.getRequestImpact(requestKey, range)) {
      this.updateBlockState(blockId, (state) => {
        state.cachedMask |= mask;
      });
    }
  }

  private updateBlockState(
    blockId: string,
    update: (state: MutableBlockState) => void,
  ): void {
    const state = this.blockState.get(blockId);
    const trackId = this.findTrackId(blockId);
    const trackState = trackId ? this.trackState.get(trackId) : undefined;
    if (!state || !trackId || !trackState) {
      return;
    }

    const wasVisible = isBlockVisible(state);
    const cachedSubpartCount = countBits(state.cachedMask);
    update(state);
    trackState.cachedSubpartCount += countBits(state.cachedMask) - cachedSubpartCount;
    if (wasVisible !== isBlockVisible(state)) {
      if (wasVisible) {
        const index = trackState.visibleBlockIds.indexOf(blockId);
        if (index !== -1) {
          trackState.visibleBlockIds.splice(index, 1);
        }
      } else {
        const blockOrder = this.blockOrderById.get(blockId) ?? 0;
        const index = lowerBoundBlockId(
          trackState.visibleBlockIds,
          blockOrder,
          this.blockOrderById,
        );
        trackState.visibleBlockIds.splice(index, 0, blockId);
      }
      trackState.visibleBlockCount = trackState.visibleBlockIds.length;
    }
    this.dirtyBlockIds.add(blockId);
    this.dirtyTrackIds.add(trackId);
  }

  private getRequestImpact(requestKey: string, range?: ByteRange): Map<string, number> {
    const existing = this.requestImpacts.get(requestKey);
    if (existing) {
      return existing;
    }
    if (!range) {
      return new Map();
    }
    const impact = this.resolveRangeImpact(range);
    this.requestImpacts.set(requestKey, impact);
    return impact;
  }

  private resolveRangeImpact(range: ByteRange): Map<string, number> {
    const impacts = new Map<string, number>();
    for (const { value: segment } of this.physicalIndex.query(range)) {
      const physicalRange = {
        start: Math.max(range.start, segment.physicalRange.start),
        end: Math.min(range.end, segment.physicalRange.end),
      };
      const logicalRange = {
        start: segment.logicalRange.start + physicalRange.start - segment.physicalRange.start,
        end: segment.logicalRange.start + physicalRange.end - segment.physicalRange.start,
      };
      const track = this.trackById.get(segment.trackId);
      if (!track) {
        continue;
      }
      for (const block of track.blocks) {
        if (!rangesOverlap(block.logicalRange, logicalRange)) {
          continue;
        }
        let mask = impacts.get(block.id) ?? 0;
        for (const subpart of block.subparts) {
          if (rangesOverlap(subpart.logicalRange, logicalRange)) {
            mask |= 1 << subpart.index;
          }
        }
        impacts.set(block.id, mask);
      }
    }
    return impacts;
  }

  private markRequestImpactDirty(requestKey: string | null): void {
    if (!requestKey) {
      return;
    }
    for (const blockId of this.getRequestImpact(requestKey).keys()) {
      this.dirtyBlockIds.add(blockId);
    }
  }

  createFileStructureSnapshot(): {
    layout: FileLayout;
    coverage: ParquetByteCoverage;
  } | null {
    return this.layout
      ? { layout: this.layout, coverage: this.coverage.clone() }
      : null;
  }

  private markSummaryDirty(): void {
    this.summaryDirty = true;
  }

  private addTrackCoverage(ranges: readonly ByteRange[]): void {
    for (const range of ranges) {
      for (const { range: segmentRange, value: segment } of this.physicalIndex.query(range)) {
        const trackState = this.trackState.get(segment.trackId);
        if (!trackState) {
          continue;
        }
        trackState.downloadedByteLength += Math.min(range.end, segmentRange.end) -
          Math.max(range.start, segmentRange.start);
        this.segmentCoveredByteLength.set(
          segment.segmentId,
          (this.segmentCoveredByteLength.get(segment.segmentId) ?? 0) +
            Math.min(range.end, segmentRange.end) -
            Math.max(range.start, segmentRange.start),
        );
        this.dirtyTrackIds.add(segment.trackId);
      }
    }
  }

  private scheduleFlush(): void {
    if (this.flushTimer !== null) {
      return;
    }
    const delay = Math.max(0, updateIntervalMs - (Date.now() - this.lastFlushTime));
    if (delay === 0) {
      this.flushDirtySnapshots();
      return;
    }
    this.flushTimer = setTimeout(() => {
      this.flushTimer = null;
      this.flushDirtySnapshots();
    }, delay);
  }

  private flushDirtySnapshots(force = false): void {
    if (
      !force &&
      !this.topologyDirty &&
      !this.summaryDirty &&
      this.dirtyBlockIds.size === 0 &&
      this.dirtyTrackIds.size === 0
    ) {
      return;
    }
    this.cancelFlush();
    this.lastFlushTime = Date.now();

    for (const blockId of this.dirtyBlockIds) {
      this.blockSnapshots.set(blockId, this.createBlockSnapshot(blockId));
    }
    for (const trackId of this.dirtyTrackIds) {
      this.trackSnapshots.set(trackId, this.createTrackSnapshot(trackId));
    }
    if (this.summaryDirty) {
      this.summarySnapshot = this.createSummarySnapshot();
    }
    if (this.topologyDirty) {
      this.topologySnapshot = this.createTopologySnapshot();
    }

    const dirtyBlockIds = [...this.dirtyBlockIds];
    const dirtyTrackIds = [...this.dirtyTrackIds];
    const notifySummary = this.summaryDirty;
    const notifyTopology = this.topologyDirty;
    this.dirtyBlockIds.clear();
    this.dirtyTrackIds.clear();
    this.summaryDirty = false;
    this.topologyDirty = false;

    if (notifyTopology) {
      notifyListeners(this.topologyListeners);
    }
    if (notifySummary) {
      notifyListeners(this.summaryListeners);
    }
    for (const blockId of dirtyBlockIds) {
      notifyListeners(this.blockListeners.get(blockId));
    }
    for (const trackId of dirtyTrackIds) {
      notifyListeners(this.trackListeners.get(trackId));
    }
  }

  private createTopologySnapshot(): DownloadTopologySnapshot {
    return {
      layout: this.layout,
      tracks: this.displayLayout?.tracks ?? [],
      rowGroupBounds: this.rowGroupBounds,
      error: this.error,
    };
  }

  private createBlockSnapshot(blockId: string): DownloadBlockSnapshot {
    const state = this.blockState.get(blockId);
    if (!state) {
      return emptyBlockSnapshot;
    }
    const loadingMask = state.loadingCount.reduce(
      (mask, count, index) => count > 0 ? mask | (1 << index) : mask,
      0,
    );
    return {
      cachedMask: state.cachedMask,
      loadingMask,
      activeMask: this.latestRequestKey
        ? this.getRequestImpact(this.latestRequestKey).get(blockId) ?? 0
        : 0,
    };
  }

  private createTrackSnapshot(trackId: string): DownloadTrackSnapshot {
    const track = this.trackById.get(trackId);
    const state = this.trackState.get(trackId);
    if (!track || !state) {
      return {
        downloadedByteLength: 0,
        filledSubpartCount: 0,
        subpartCount: 0,
        visibleBlockCount: 0,
        visibleBlockIds: [],
      };
    }
    return {
      downloadedByteLength: state.downloadedByteLength,
      filledSubpartCount: state.cachedSubpartCount,
      subpartCount: track.blocks.length * displaySubpartCount,
      visibleBlockCount: state.visibleBlockCount,
      visibleBlockIds: [...state.visibleBlockIds],
    };
  }

  private createSummarySnapshot(): DownloadSummarySnapshot {
    const tracks = this.displayLayout?.tracks ?? [];
    const columns = tracks.filter((track) => track.kind === "column");
    return {
      downloadedByteLength: this.downloadedByteLength,
      completedRequestCount: this.completedRequestCount,
      cachedSubpartCount: tracks.reduce(
        (total, track) => total + (this.trackState.get(track.id)?.cachedSubpartCount ?? 0),
        0,
      ),
      subpartCount: tracks.reduce(
        (total, track) => total + (this.trackSnapshots.get(track.id)?.subpartCount ?? 0),
        0,
      ),
      visibleColumnCount: columns.filter(
        (track) => (this.trackState.get(track.id)?.visibleBlockCount ?? 0) > 0,
      ).length,
      columnCount: columns.length,
    };
  }

  private findTrackId(blockId: string): string | null {
    const separator = blockId.lastIndexOf(":block:");
    return separator === -1 ? null : blockId.slice(0, separator);
  }

  private cancelFlush(): void {
    if (this.flushTimer !== null) {
      clearTimeout(this.flushTimer);
      this.flushTimer = null;
    }
  }
}

function createRequestKey(
  event: Pick<ArcgisParquetRangeReadEvent, "fileId" | "requestId">,
): string {
  return `${encodeURIComponent(event.fileId)}:${event.requestId}`;
}

function lowerBoundBlockId(
  blockIds: readonly string[],
  blockOrder: number,
  blockOrderById: ReadonlyMap<string, number>,
): number {
  let lower = 0;
  let upper = blockIds.length;
  while (lower < upper) {
    const middle = Math.floor((lower + upper) / 2);
    if ((blockOrderById.get(blockIds[middle]) ?? 0) < blockOrder) {
      lower = middle + 1;
    } else {
      upper = middle;
    }
  }
  return lower;
}

function isBlockVisible(state: MutableBlockState): boolean {
  return state.cachedMask !== 0 || state.loadingCount.some((count) => count > 0);
}

function countBits(mask: number): number {
  let value = mask;
  let count = 0;
  while (value !== 0) {
    count += value & 1;
    value >>>= 1;
  }
  return count;
}

function notifyListeners(listeners: ReadonlySet<() => void> | undefined): void {
  for (const listener of listeners ?? []) {
    listener();
  }
}

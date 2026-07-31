import { ByteRangeIndex } from "../../../parquet/ByteRangeIndex";
import {
  type DownloadBlockLayout,
  type DownloadDisplayLayout,
  type DownloadPhysicalSegment,
  type DownloadTrackLayout,
  displaySubpartCount,
} from "../../../parquet/displayLayout";
import {
  type ByteRange,
  type FileLayout,
  rangesOverlap,
} from "../../../parquet/fileLayout";
import type { RowGroupBounds } from "../../../parquet/rowGroupBounds";
import type {
  DownloadBlockSnapshot,
  DownloadColumnStatistics,
  DownloadRowGroupCoverage,
  DownloadRowGroupItemCoverage,
  DownloadSummarySnapshot,
  DownloadTopologySnapshot,
  DownloadTrackSnapshot,
} from "./types";

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

export interface ProjectionChange {
  blockIds: readonly string[];
  trackIds: readonly string[];
}

export interface ProjectionTopologyInput {
  layout: FileLayout | null;
  rowGroupBounds: readonly RowGroupBounds[] | null;
  error: Error | null;
}

interface MutableProjectionChange {
  blockIds: Set<string>;
  trackIds: Set<string>;
}

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
 * Represents download activity as the block, track, and row-group state consumed by the file explorer.
 * It owns byte-to-display indexes and request masks so rendering code never translates physical ranges itself.
 */
export class ParquetDownloadProjection {
  private _displayLayout: DownloadDisplayLayout | null = null;
  private _physicalIndex = new ByteRangeIndex<DownloadPhysicalSegment>([]);
  private readonly _blockById = new Map<string, DownloadBlockLayout>();
  private readonly _blockOrderById = new Map<string, number>();
  private readonly _trackById = new Map<string, DownloadTrackLayout>();
  private readonly _blockState = new Map<string, MutableBlockState>();
  private readonly _trackState = new Map<string, MutableTrackState>();
  private readonly _segmentCoveredByteLength = new Map<string, number>();
  private readonly _requestImpacts = new Map<string, Map<string, number>>();

  reset(): void {
    this._displayLayout = null;
    this._requestImpacts.clear();
    this._clearDisplayState();
  }

  initialize(displayLayout: DownloadDisplayLayout): ProjectionChange {
    this._displayLayout = displayLayout;
    this._clearDisplayState();
    this._physicalIndex = new ByteRangeIndex(
      displayLayout.segments.map((segment) => ({
        range: segment.physicalRange,
        value: segment,
      })),
    );

    const trackIds: string[] = [];
    for (const track of displayLayout.tracks) {
      this._trackById.set(track.id, track);
      this._trackState.set(track.id, {
        downloadedByteLength: 0,
        cachedSubpartCount: 0,
        visibleBlockCount: 0,
        visibleBlockIds: [],
      });
      trackIds.push(track.id);
      for (const [blockIndex, block] of track.blocks.entries()) {
        this._blockById.set(block.id, block);
        this._blockOrderById.set(block.id, blockIndex);
        this._blockState.set(block.id, {
          cachedMask: 0,
          loadingCount: Array<number>(displaySubpartCount).fill(0),
        });
      }
      for (const segment of track.segments) {
        this._segmentCoveredByteLength.set(segment.segmentId, 0);
      }
    }
    return { blockIds: [], trackIds };
  }

  applyCachedRange(range: ByteRange): ProjectionChange {
    const change = createProjectionChange();
    for (const [blockId, mask] of this._resolveRangeImpact(range)) {
      this._updateBlockState(blockId, change, (state) => {
        state.cachedMask |= mask;
      });
    }
    return freezeProjectionChange(change);
  }

  applyCachedRequestImpact(
    requestKey: string,
    range: ByteRange,
  ): ProjectionChange {
    const change = createProjectionChange();
    for (const [blockId, mask] of this._getRequestImpact(requestKey, range)) {
      this._updateBlockState(blockId, change, (state) => {
        state.cachedMask |= mask;
      });
    }
    return freezeProjectionChange(change);
  }

  applyLoadingRequestImpact(
    requestKey: string,
    range: ByteRange | undefined,
    direction: 1 | -1,
  ): ProjectionChange {
    const change = createProjectionChange();
    for (const [blockId, mask] of this._getRequestImpact(requestKey, range)) {
      this._updateBlockState(blockId, change, (state) => {
        for (let index = 0; index < displaySubpartCount; index += 1) {
          if ((mask & (1 << index)) !== 0) {
            state.loadingCount[index] = Math.max(
              0,
              state.loadingCount[index] + direction,
            );
          }
        }
      });
    }
    return freezeProjectionChange(change);
  }

  addCoveredRanges(ranges: readonly ByteRange[]): ProjectionChange {
    const change = createProjectionChange();
    for (const range of ranges) {
      for (const { range: segmentRange, value: segment } of this._physicalIndex.query(range)) {
        const trackState = this._trackState.get(segment.trackId);
        if (!trackState) {
          continue;
        }
        const overlapByteLength = Math.min(range.end, segmentRange.end) -
          Math.max(range.start, segmentRange.start);
        trackState.downloadedByteLength += overlapByteLength;
        this._segmentCoveredByteLength.set(
          segment.segmentId,
          (this._segmentCoveredByteLength.get(segment.segmentId) ?? 0) +
            overlapByteLength,
        );
        change.trackIds.add(segment.trackId);
      }
    }
    return freezeProjectionChange(change);
  }

  requestImpactBlockIds(requestKey: string | null): readonly string[] {
    return requestKey ? [...this._getRequestImpact(requestKey).keys()] : [];
  }

  createBlockSnapshot(
    blockId: string,
    activeRequestKey: string | null,
  ): DownloadBlockSnapshot {
    const state = this._blockState.get(blockId);
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
      activeMask: activeRequestKey
        ? this._getRequestImpact(activeRequestKey).get(blockId) ?? 0
        : 0,
    };
  }

  createTrackSnapshot(trackId: string): DownloadTrackSnapshot {
    const track = this._trackById.get(trackId);
    const state = this._trackState.get(trackId);
    if (!track || !state) {
      return emptyTrackSnapshot;
    }
    return {
      downloadedByteLength: state.downloadedByteLength,
      filledSubpartCount: state.cachedSubpartCount,
      subpartCount: track.blocks.length * displaySubpartCount,
      visibleBlockCount: state.visibleBlockCount,
      visibleBlockIds: [...state.visibleBlockIds],
    };
  }

  createSummarySnapshot(
    downloadedByteLength: number,
    completedRequestCount: number,
  ): DownloadSummarySnapshot {
    const tracks = this._displayLayout?.tracks ?? [];
    const columns = tracks.filter((track) => track.kind === "column");
    return {
      downloadedByteLength,
      completedRequestCount,
      cachedSubpartCount: tracks.reduce(
        (total, track) => total + (this._trackState.get(track.id)?.cachedSubpartCount ?? 0),
        0,
      ),
      subpartCount: tracks.reduce(
        (total, track) => total + track.blocks.length * displaySubpartCount,
        0,
      ),
      visibleColumnCount: columns.filter(
        (track) => (this._trackState.get(track.id)?.visibleBlockCount ?? 0) > 0,
      ).length,
      columnCount: columns.length,
    };
  }

  createTopologySnapshot({
    layout,
    rowGroupBounds,
    error,
  }: ProjectionTopologyInput): DownloadTopologySnapshot {
    return {
      layout,
      tracks: this._displayLayout?.tracks ?? [],
      rowGroupBounds,
      error,
    };
  }

  rowGroupCoverage(trackId: string): readonly DownloadRowGroupCoverage[] {
    const track = this._trackById.get(trackId);
    if (!track) {
      return [];
    }

    const coverageByRowGroup = new Map<number, {
      rowGroupIndex: number;
      fileId?: number;
      fileName?: string;
      sourceRowGroupIndex?: number;
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
      const coveredByteLength = this._segmentCoveredByteLength.get(segment.segmentId) ?? 0;
      const rowGroupCoverage = coverageByRowGroup.get(segment.rowGroupIndex) ?? {
        rowGroupIndex: segment.rowGroupIndex,
        fileId: segment.fileId,
        fileName: segment.fileName,
        sourceRowGroupIndex: segment.sourceRowGroupIndex,
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
      const { itemCoverageByLabel, ...coverage } = rowGroupCoverage;
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

  rowGroupBlockMasks(
    trackId: string,
    rowGroupIndex: number,
    overlaps: (range: ByteRange) => boolean,
  ): ReadonlyMap<string, number> {
    const track = this._trackById.get(trackId);
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
            overlaps(piece.physicalRange),
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

  private _clearDisplayState(): void {
    this._physicalIndex = new ByteRangeIndex([]);
    this._blockById.clear();
    this._blockOrderById.clear();
    this._trackById.clear();
    this._blockState.clear();
    this._trackState.clear();
    this._segmentCoveredByteLength.clear();
  }

  private _updateBlockState(
    blockId: string,
    change: MutableProjectionChange,
    update: (state: MutableBlockState) => void,
  ): void {
    const state = this._blockState.get(blockId);
    const trackId = findTrackId(blockId);
    const trackState = trackId ? this._trackState.get(trackId) : undefined;
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
        const blockOrder = this._blockOrderById.get(blockId) ?? 0;
        const index = lowerBoundBlockId(
          trackState.visibleBlockIds,
          blockOrder,
          this._blockOrderById,
        );
        trackState.visibleBlockIds.splice(index, 0, blockId);
      }
      trackState.visibleBlockCount = trackState.visibleBlockIds.length;
    }
    change.blockIds.add(blockId);
    change.trackIds.add(trackId);
  }

  private _getRequestImpact(
    requestKey: string,
    range?: ByteRange,
  ): Map<string, number> {
    const existing = this._requestImpacts.get(requestKey);
    if (existing) {
      return existing;
    }
    if (!range) {
      return new Map();
    }
    const impact = this._resolveRangeImpact(range);
    this._requestImpacts.set(requestKey, impact);
    return impact;
  }

  private _resolveRangeImpact(range: ByteRange): Map<string, number> {
    const impacts = new Map<string, number>();
    for (const { value: segment } of this._physicalIndex.query(range)) {
      const physicalRange = {
        start: Math.max(range.start, segment.physicalRange.start),
        end: Math.min(range.end, segment.physicalRange.end),
      };
      const logicalRange = {
        start: segment.logicalRange.start + physicalRange.start - segment.physicalRange.start,
        end: segment.logicalRange.start + physicalRange.end - segment.physicalRange.start,
      };
      const track = this._trackById.get(segment.trackId);
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
}

function createProjectionChange(): MutableProjectionChange {
  return {
    blockIds: new Set(),
    trackIds: new Set(),
  };
}

function freezeProjectionChange(change: MutableProjectionChange): ProjectionChange {
  return {
    blockIds: [...change.blockIds],
    trackIds: [...change.trackIds],
  };
}

function findTrackId(blockId: string): string | null {
  const separator = blockId.lastIndexOf(":block:");
  return separator === -1 ? null : blockId.slice(0, separator);
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

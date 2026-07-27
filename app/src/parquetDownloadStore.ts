import {
  type ByteRange,
  type FileLayout,
  type PageIndexLayout,
  type RangeLifecycleObserver,
  type SyntheticBlock,
  rangesOverlap,
  syntheticBlockSize,
  syntheticSubchunkSize,
} from "./parquetFileLayout";

export type BlockState = "empty" | "loading" | "cached" | "active" | "failed";

export interface DownloadSnapshot {
  layout: FileLayout | null;
  blockStates: ReadonlyMap<string, BlockState>;
  blockDownloadBackgrounds: ReadonlyMap<string, string>;
  blockDownloadFractions: ReadonlyMap<string, number>;
  visibleColumns: readonly DownloadColumn[];
  fieldCount: number;
  footerBlocks: readonly DownloadMetadataBlock[];
  indexSummary: DownloadIndexSummary | null;
  downloadedByteLength: number;
  completedRequestCount: number;
  cachedSubchunkCount: number;
  subchunkCount: number;
  error: Error | null;
}

interface ActiveRequest {
  range: ByteRange;
  state: "loading" | "failed";
}

interface IndexedColumn {
  id: string;
  fieldName: string;
  byteRange: ByteRange;
}

export interface DownloadBlock {
  id: string;
  byteRange: ByteRange;
  rowGroupIndex: number;
}

export interface DownloadColumn {
  fieldName: string;
  blocks: DownloadBlock[];
  downloadedByteLength: number;
  byteLength: number;
}

export interface DownloadIndexSummary {
  byteLength: number;
  downloadedByteLength: number;
  state: BlockState;
  background: string;
  blocks: DownloadIndexBlock[];
}

export interface DownloadMetadataBlock {
  id: string;
  byteRange: ByteRange;
  state: BlockState;
  background: string;
}

export interface DownloadIndexBlock {
  id: string;
  byteLength: number;
  downloadedByteLength: number;
  details: DownloadIndexDetail[];
  state: BlockState;
  background: string;
}

export interface DownloadIndexDetail {
  rowGroupIndex: number;
  fieldName: string;
  kind: PageIndexLayout["kind"];
  byteRange: ByteRange;
  byteLength: number;
  downloadedByteLength: number;
}

export class ParquetDownloadStore implements RangeLifecycleObserver {
  private layout: FileLayout | null = null;
  private completedRanges: ByteRange[] = [];
  private activeRequests = new Map<string, ActiveRequest>();
  private latestRequestId: string | null = null;
  private error: Error | null = null;
  private nextRequestId = 0;
  private listeners = new Set<() => void>();
  private readonly blockStates = new Map<string, BlockState>();
  private readonly blockDownloadBackgrounds = new Map<string, string>();
  private readonly blockDownloadFractions = new Map<string, number>();
  private blocks: SyntheticBlock[] = [];
  private indexedColumns: IndexedColumn[] = [];
  private indexRanges: ByteRange[] = [];
  private indexEntries: PageIndexLayout[] = [];
  private displayColumns: DownloadColumn[] = [];
  private visibleColumns: DownloadColumn[] = [];
  private readonly displayColumnByBlockId = new Map<string, DownloadColumn>();
  private readonly displayColumnByIndexedColumnId = new Map<string, DownloadColumn>();
  private readonly displayBlockById = new Map<string, DownloadBlock>();
  private fieldCount = 0;
  private downloadedByteLength = 0;
  private completedRequestCount = 0;
  private indexByteLength = 0;
  private downloadedIndexByteLength = 0;
  private cachedSubchunkCount = 0;
  private subchunkCount = 0;
  private cachedSubchunkIds = new Set<string>();
  private snapshot: DownloadSnapshot = this.createSnapshot();

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getSnapshot = (): DownloadSnapshot => this.snapshot;

  reset(): void {
    this.layout = null;
    this.completedRanges = [];
    this.activeRequests.clear();
    this.latestRequestId = null;
    this.error = null;
    this.blockStates.clear();
    this.blockDownloadBackgrounds.clear();
    this.blockDownloadFractions.clear();
    this.blocks = [];
    this.indexedColumns = [];
    this.indexRanges = [];
    this.indexEntries = [];
    this.displayColumns = [];
    this.visibleColumns = [];
    this.displayColumnByBlockId.clear();
    this.displayColumnByIndexedColumnId.clear();
    this.displayBlockById.clear();
    this.fieldCount = 0;
    this.downloadedByteLength = 0;
    this.completedRequestCount = 0;
    this.indexByteLength = 0;
    this.downloadedIndexByteLength = 0;
    this.cachedSubchunkCount = 0;
    this.subchunkCount = 0;
    this.cachedSubchunkIds.clear();
    this.publish();
  }

  setLayout(layout: FileLayout): void {
    this.layout = layout;
    this.blocks = layout.rowGroups
      .flatMap((rowGroup) => rowGroup.columns.flatMap((column) => column.blocks))
      .sort((first, second) => first.byteRange.start - second.byteRange.start);
    this.indexedColumns = layout.rowGroups
      .flatMap((rowGroup) => rowGroup.columns.map((column, columnIndex) => ({
          id: `${rowGroup.index}-${columnIndex}`,
          fieldName: column.fieldName,
          byteRange: column.byteRange,
        })))
      .sort((first, second) => first.byteRange.start - second.byteRange.start);
    this.indexEntries = [...layout.pageIndexes].sort(
      (first, second) => first.byteRange.start - second.byteRange.start,
    );
    this.indexRanges = mergeRanges(this.indexEntries.map((pageIndex) => pageIndex.byteRange));
    this.indexByteLength = totalByteLength(this.indexRanges);
    this.downloadedIndexByteLength = 0;
    this.initializeDisplayColumns(layout);
    for (const completedRange of this.completedRanges) {
      this.updateDownloadedIndexByteLength(completedRange);
    }
    this.subchunkCount = this.indexedColumns.reduce(
      (total, column) =>
        total + Math.ceil((column.byteRange.end - column.byteRange.start) / syntheticSubchunkSize),
      0,
    );
    this.refreshBlocks([...this.completedRanges, ...this.activeRequestRanges()], true);
    this.recalculateCachedSubchunkCount();
    this.publish();
  }

  setError(error: Error): void {
    this.error = error;
    this.publish();
  }

  startRange(range: ByteRange): string {
    const previousLatestRange = this.latestRequestId
      ? this.activeRequests.get(this.latestRequestId)?.range
      : undefined;
    const requestId = `range-${this.nextRequestId}`;
    this.nextRequestId += 1;
    this.latestRequestId = requestId;
    this.activeRequests.set(requestId, { range, state: "loading" });
    this.refreshBlocks([range, ...(previousLatestRange ? [previousLatestRange] : [])], false);
    this.publish();

    return requestId;
  }

  completeRange(requestId: string): void {
    const request = this.activeRequests.get(requestId);
    if (!request) {
      return;
    }

    this.activeRequests.delete(requestId);
    this.completedRequestCount += 1;
    const newlyDownloadedRanges = uncoveredRanges(request.range, this.completedRanges);
    for (const downloadedRange of newlyDownloadedRanges) {
      this.updateColumnDownloadedByteLength(downloadedRange);
      this.updateDownloadedIndexByteLength(downloadedRange);
    }
    this.downloadedByteLength += this.addCompletedRange(request.range);
    this.refreshBlocks([request.range], true);
    this.updateCachedSubchunkCount(request.range);
    this.publish();
  }

  failRange(requestId: string): void {
    const request = this.activeRequests.get(requestId);
    if (!request) {
      return;
    }

    request.state = "failed";
    this.refreshBlocks([request.range], false);
    this.publish();
  }

  private publish(): void {
    this.snapshot = this.createSnapshot();
    for (const listener of this.listeners) {
      listener();
    }
  }

  private createSnapshot(): DownloadSnapshot {
    return {
      layout: this.layout,
      blockStates: this.blockStates,
      blockDownloadBackgrounds: this.blockDownloadBackgrounds,
      blockDownloadFractions: this.blockDownloadFractions,
      visibleColumns: this.visibleColumns,
      fieldCount: this.fieldCount,
      footerBlocks: this.createFooterBlocks(),
      indexSummary: this.createIndexSummary(),
      downloadedByteLength: this.downloadedByteLength,
      completedRequestCount: this.completedRequestCount,
      cachedSubchunkCount: this.cachedSubchunkCount,
      subchunkCount: this.subchunkCount,
      error: this.error,
    };
  }

  private createIndexSummary(): DownloadIndexSummary | null {
    const state = this.getIndexState();
    if (state === "empty") {
      return null;
    }

    let aggregateStart = 0;
    const blocks = splitByteLength(this.indexByteLength).flatMap((byteLength, index) => {
      const aggregateEnd = aggregateStart + byteLength;
      const details = mapAggregateRangeToIndexDetails(
        { start: aggregateStart, end: aggregateEnd },
        this.indexEntries,
        this.completedRanges,
      );
      aggregateStart = aggregateEnd;
      const downloadedByteLength = details.reduce(
        (total, detail) => total + detail.downloadedByteLength,
        0,
      );

      return downloadedByteLength > 0
        ? [
            {
              id: `index-${index}`,
              byteLength,
              downloadedByteLength,
              details,
              state: getAggregateBlockState(state, downloadedByteLength),
              background: aggregateFillBackground(downloadedByteLength / byteLength),
            },
          ]
        : [];
    });

    return {
      byteLength: this.indexByteLength,
      downloadedByteLength: this.downloadedIndexByteLength,
      state,
      background: aggregateFillBackground(
        this.downloadedIndexByteLength / this.indexByteLength,
      ),
      blocks,
    };
  }

  private createFooterBlocks(): DownloadMetadataBlock[] {
    if (!this.layout) {
      return [];
    }

    return splitByteRange(this.layout.footer).map((byteRange, index) => ({
      id: `footer-${index}`,
      byteRange,
      state: this.getRangeState(byteRange),
      background: rangeFillBackground(byteRange, this.completedRanges),
    }));
  }

  private refreshBlocks(ranges: ByteRange[], includeFill: boolean): void {
    const affectedBlocks = new Map<string, SyntheticBlock>();
    const displayColumnsToSort = new Set<DownloadColumn>();
    let visibilityChanged = false;

    for (const range of ranges) {
      for (const block of this.blocksOverlapping(range)) {
        affectedBlocks.set(block.id, block);
      }
    }

    for (const block of affectedBlocks.values()) {
      const previousState = this.blockStates.get(block.id) ?? "empty";
      const state = this.getRangeState(block.byteRange);
      if (state === "empty") {
        this.blockStates.delete(block.id);
      } else {
        this.blockStates.set(block.id, state);
      }

      if (includeFill) {
        const fraction = rangeCoverageFraction(block.byteRange, this.completedRanges);
        if (fraction === 0) {
          this.blockDownloadBackgrounds.delete(block.id);
          this.blockDownloadFractions.delete(block.id);
        } else {
          this.blockDownloadBackgrounds.set(
            block.id,
            rangeFillBackground(block.byteRange, this.completedRanges),
          );
          this.blockDownloadFractions.set(block.id, fraction);
        }

      }

      const wasVisible = isVisibleBlockState(previousState);
      const isVisible = isVisibleBlockState(state);
      if (wasVisible !== isVisible) {
        const column = this.displayColumnByBlockId.get(block.id);
        const displayBlock = this.displayBlockById.get(block.id);
        if (column) {
          if (isVisible && displayBlock) {
            column.blocks.push(displayBlock);
            displayColumnsToSort.add(column);
          } else {
            const index = column.blocks.findIndex((candidate) => candidate.id === block.id);
            if (index !== -1) {
              column.blocks.splice(index, 1);
            }
          }
          visibilityChanged ||= column.blocks.length === 1 || column.blocks.length === 0;
        }
      }
    }

    for (const column of displayColumnsToSort) {
      column.blocks.sort((first, second) =>
        first.rowGroupIndex === second.rowGroupIndex
          ? first.byteRange.start - second.byteRange.start
          : first.rowGroupIndex - second.rowGroupIndex,
      );
    }

    if (visibilityChanged) {
      this.visibleColumns = this.displayColumns.filter((column) => column.blocks.length > 0);
    }
  }

  private *blocksOverlapping(range: ByteRange): Iterable<SyntheticBlock> {
    const firstIndex = lowerBoundByStart(
      this.blocks,
      range.start - syntheticBlockSize,
      (block) => block.byteRange.start,
    );

    for (let index = firstIndex; index < this.blocks.length; index += 1) {
      const block = this.blocks[index];
      if (block.byteRange.start >= range.end) {
        break;
      }
      if (rangesOverlap(block.byteRange, range)) {
        yield block;
      }
    }
  }

  private activeRequestRanges(): ByteRange[] {
    return Array.from(this.activeRequests.values(), (request) => request.range);
  }

  private addCompletedRange(range: ByteRange): number {
    let index = lowerBoundByStart(this.completedRanges, range.start, (completedRange) => completedRange.start);
    if (index > 0 && this.completedRanges[index - 1].end >= range.start) {
      index -= 1;
    }

    let mergedRange = { ...range };
    let replacedByteLength = 0;
    let count = 0;
    while (
      index + count < this.completedRanges.length &&
      this.completedRanges[index + count].start <= mergedRange.end
    ) {
      const existingRange = this.completedRanges[index + count];
      mergedRange = {
        start: Math.min(mergedRange.start, existingRange.start),
        end: Math.max(mergedRange.end, existingRange.end),
      };
      replacedByteLength += existingRange.end - existingRange.start;
      count += 1;
    }

    this.completedRanges.splice(index, count, mergedRange);
    return mergedRange.end - mergedRange.start - replacedByteLength;
  }

  private recalculateCachedSubchunkCount(): void {
    this.cachedSubchunkIds.clear();
    this.cachedSubchunkCount = 0;

    for (const column of this.indexedColumns) {
      this.cacheCoveredSubchunks(column, column.byteRange);
    }
  }

  private updateCachedSubchunkCount(range: ByteRange): void {
    for (const column of this.columnsOverlapping(range)) {
      this.cacheCoveredSubchunks(column, range);
    }
  }

  private updateColumnDownloadedByteLength(range: ByteRange): void {
    for (const column of this.columnsOverlapping(range)) {
      const displayColumn = this.displayColumnByIndexedColumnId.get(column.id);
      if (displayColumn) {
        displayColumn.downloadedByteLength +=
          Math.min(column.byteRange.end, range.end) - Math.max(column.byteRange.start, range.start);
      }
    }
  }

  private updateDownloadedIndexByteLength(range: ByteRange): void {
    this.downloadedIndexByteLength += overlappingByteLength(range, this.indexRanges);
  }

  private *columnsOverlapping(range: ByteRange): Iterable<IndexedColumn> {
    let firstIndex = lowerBoundByStart(
      this.indexedColumns,
      range.start,
      (column) => column.byteRange.start,
    );
    if (firstIndex > 0) {
      firstIndex -= 1;
    }

    for (let index = firstIndex; index < this.indexedColumns.length; index += 1) {
      const column = this.indexedColumns[index];
      if (column.byteRange.start >= range.end) {
        break;
      }
      if (rangesOverlap(column.byteRange, range)) {
        yield column;
      }
    }
  }

  private cacheCoveredSubchunks(column: IndexedColumn, range: ByteRange): void {
    const startOffset = Math.max(0, range.start - column.byteRange.start);
    const firstSubchunkStart =
      column.byteRange.start +
      Math.floor(startOffset / syntheticSubchunkSize) * syntheticSubchunkSize;
    const finalSubchunkEnd = Math.min(column.byteRange.end, range.end);

    for (let start = firstSubchunkStart; start < finalSubchunkEnd; start += syntheticSubchunkSize) {
      const end = Math.min(start + syntheticSubchunkSize, column.byteRange.end);
      if (rangeCoverageFraction({ start, end }, this.completedRanges) === 1) {
        const id = `${column.id}:${start}`;
        if (!this.cachedSubchunkIds.has(id)) {
          this.cachedSubchunkIds.add(id);
          this.cachedSubchunkCount += 1;
        }
      }
    }
  }

  private initializeDisplayColumns(layout: FileLayout): void {
    this.displayColumns = [];
    this.visibleColumns = [];
    this.displayColumnByBlockId.clear();
    this.displayColumnByIndexedColumnId.clear();
    this.displayBlockById.clear();
    const columnByField = new Map<string, DownloadColumn>();

    for (const rowGroup of layout.rowGroups) {
      for (const column of rowGroup.columns) {
        const displayColumn = columnByField.get(column.fieldName) ?? {
          fieldName: column.fieldName,
          blocks: [],
          downloadedByteLength: 0,
          byteLength: 0,
        };
        displayColumn.byteLength += column.byteRange.end - column.byteRange.start;
        columnByField.set(column.fieldName, displayColumn);
        for (const block of column.blocks) {
          this.displayColumnByBlockId.set(block.id, displayColumn);
          this.displayBlockById.set(block.id, {
            id: block.id,
            byteRange: block.byteRange,
            rowGroupIndex: rowGroup.index,
          });
        }
      }
    }

    this.displayColumns = Array.from(columnByField.values());
    for (const indexedColumn of this.indexedColumns) {
      const displayColumn = columnByField.get(indexedColumn.fieldName);
      if (displayColumn) {
        this.displayColumnByIndexedColumnId.set(indexedColumn.id, displayColumn);
      }
    }

    this.fieldCount = this.displayColumns.length;

    for (const completedRange of this.completedRanges) {
      this.updateColumnDownloadedByteLength(completedRange);
    }
  }

  private getRangeState(range: ByteRange): BlockState {
    for (const request of this.activeRequests.values()) {
      if (request.state === "failed" && rangesOverlap(range, request.range)) {
        return "failed";
      }
    }

    const latestRequest = this.latestRequestId ? this.activeRequests.get(this.latestRequestId) : undefined;
    if (latestRequest?.state === "loading" && rangesOverlap(range, latestRequest.range)) {
      return "active";
    }

    for (const request of this.activeRequests.values()) {
      if (request.state === "loading" && rangesOverlap(range, request.range)) {
        return "loading";
      }
    }

    return hasOverlappingRange(range, this.completedRanges)
      ? "cached"
      : "empty";
  }

  private getIndexState(): BlockState {
    for (const request of this.activeRequests.values()) {
      if (request.state === "failed" && hasOverlappingRange(request.range, this.indexRanges)) {
        return "failed";
      }
    }

    const latestRequest = this.latestRequestId ? this.activeRequests.get(this.latestRequestId) : undefined;
    if (latestRequest?.state === "loading" && hasOverlappingRange(latestRequest.range, this.indexRanges)) {
      return "active";
    }

    for (const request of this.activeRequests.values()) {
      if (request.state === "loading" && hasOverlappingRange(request.range, this.indexRanges)) {
        return "loading";
      }
    }

    return this.downloadedIndexByteLength > 0 ? "cached" : "empty";
  }
}

function isVisibleBlockState(state: BlockState): boolean {
  return state === "loading" || state === "active" || state === "cached";
}

function rangeCoverageFraction(range: ByteRange, completedRanges: ByteRange[]): number {
  const overlappingRanges = overlappingRangeSet(range, completedRanges);

  return overlappingRanges.reduce((total, overlap) => total + overlap.end - overlap.start, 0) /
    (range.end - range.start);
}

const emptyBlockFill = "var(--calcite-color-foreground-3)";
const cachedBlockFill =
  "color-mix(in srgb, var(--calcite-color-brand) 85%, white)";

function rangeFillBackground(range: ByteRange, completedRanges: ByteRange[]): string {
  const stops: string[] = [];
  let cursor = range.start;

  for (const completedRange of overlappingRangeSet(range, completedRanges)) {
    if (cursor < completedRange.start) {
      stops.push(
        `${emptyBlockFill} ${rangePosition(cursor, range)} ${rangePosition(completedRange.start, range)}`,
      );
    }

    stops.push(
      `${cachedBlockFill} ${rangePosition(completedRange.start, range)} ${rangePosition(completedRange.end, range)}`,
    );
    cursor = completedRange.end;
  }

  if (cursor < range.end) {
    stops.push(`${emptyBlockFill} ${rangePosition(cursor, range)} 100%`);
  }

  return createBlockCoverageBackground(
    stops.join(", ") || `${emptyBlockFill} 0% 100%`,
  );
}

function overlappingRangeSet(range: ByteRange, completedRanges: ByteRange[]): ByteRange[] {
  let index = lowerBoundByStart(completedRanges, range.start, (completedRange) => completedRange.start);
  if (index > 0 && completedRanges[index - 1].end > range.start) {
    index -= 1;
  }

  const overlaps: ByteRange[] = [];
  for (; index < completedRanges.length; index += 1) {
    const completedRange = completedRanges[index];
    if (completedRange.start >= range.end) {
      break;
    }

    const start = Math.max(range.start, completedRange.start);
    const end = Math.min(range.end, completedRange.end);
    if (start < end) {
      overlaps.push({ start, end });
    }
  }

  return overlaps;
}

function hasOverlappingRange(range: ByteRange, completedRanges: ByteRange[]): boolean {
  let index = lowerBoundByStart(completedRanges, range.start, (completedRange) => completedRange.start);
  if (index > 0 && completedRanges[index - 1].end > range.start) {
    index -= 1;
  }

  return index < completedRanges.length && rangesOverlap(range, completedRanges[index]);
}

function aggregateFillBackground(fraction: number): string {
  const filledPercent = `${Math.round(fraction * 10000) / 100}%`;
  return createBlockCoverageBackground(
    `${cachedBlockFill} 0 ${filledPercent}, ${emptyBlockFill} ${filledPercent} 100%`,
  );
}

function createBlockCoverageBackground(stops: string): string {
  return [
    "linear-gradient(to bottom, rgb(255 255 255 / 8%), transparent 55%)",
    `linear-gradient(to right, ${stops})`,
  ].join(", ");
}

function getAggregateBlockState(
  aggregateState: BlockState,
  downloadedByteLength: number,
): BlockState {
  if (downloadedByteLength > 0) {
    return "cached";
  }

  return aggregateState === "cached" ? "empty" : aggregateState;
}

function overlappingByteLength(range: ByteRange, ranges: ByteRange[]): number {
  return overlappingRangeSet(range, ranges).reduce(
    (total, overlap) => total + overlap.end - overlap.start,
    0,
  );
}

function mergeRanges(ranges: ByteRange[]): ByteRange[] {
  const mergedRanges: ByteRange[] = [];

  for (const range of [...ranges].sort((first, second) => first.start - second.start)) {
    const previousRange = mergedRanges.at(-1);
    if (previousRange && range.start <= previousRange.end) {
      previousRange.end = Math.max(previousRange.end, range.end);
    } else {
      mergedRanges.push({ ...range });
    }
  }

  return mergedRanges;
}

function totalByteLength(ranges: ByteRange[]): number {
  return ranges.reduce((total, range) => total + range.end - range.start, 0);
}

function mapAggregateRangeToIndexDetails(
  aggregateRange: ByteRange,
  indexEntries: PageIndexLayout[],
  completedRanges: ByteRange[],
): DownloadIndexDetail[] {
  const details: DownloadIndexDetail[] = [];
  let aggregateStart = 0;

  for (const indexEntry of indexEntries) {
    const indexRange = indexEntry.byteRange;
    const aggregateEnd = aggregateStart + indexRange.end - indexRange.start;
    const overlapStart = Math.max(aggregateRange.start, aggregateStart);
    const overlapEnd = Math.min(aggregateRange.end, aggregateEnd);

    if (overlapStart < overlapEnd) {
      const byteRange = {
        start: indexRange.start + overlapStart - aggregateStart,
        end: indexRange.start + overlapEnd - aggregateStart,
      };
      const downloadedByteLength = overlappingByteLength(byteRange, completedRanges);
      if (downloadedByteLength > 0) {
        details.push({
          rowGroupIndex: indexEntry.rowGroupIndex,
          fieldName: indexEntry.fieldName,
          kind: indexEntry.kind,
          byteRange,
          byteLength: byteRange.end - byteRange.start,
          downloadedByteLength,
        });
      }
    }

    if (aggregateEnd >= aggregateRange.end) {
      break;
    }
    aggregateStart = aggregateEnd;
  }

  return details;
}

function splitByteRange(range: ByteRange): ByteRange[] {
  return splitByteLength(range.end - range.start).map((byteLength, index) => {
    const start = range.start + index * syntheticBlockSize;
    return { start, end: start + byteLength };
  });
}

function splitByteLength(byteLength: number): number[] {
  const blockByteLengths: number[] = [];

  for (let remainingByteLength = byteLength; remainingByteLength > 0;) {
    const blockByteLength = Math.min(remainingByteLength, syntheticBlockSize);
    blockByteLengths.push(blockByteLength);
    remainingByteLength -= blockByteLength;
  }

  return blockByteLengths;
}

function uncoveredRanges(range: ByteRange, completedRanges: ByteRange[]): ByteRange[] {
  const uncovered: ByteRange[] = [];
  let cursor = range.start;

  for (const completedRange of overlappingRangeSet(range, completedRanges)) {
    if (cursor < completedRange.start) {
      uncovered.push({ start: cursor, end: completedRange.start });
    }
    cursor = completedRange.end;
  }

  if (cursor < range.end) {
    uncovered.push({ start: cursor, end: range.end });
  }

  return uncovered;
}

function rangePosition(position: number, range: ByteRange): string {
  return `${((position - range.start) / (range.end - range.start)) * 100}%`;
}

function lowerBoundByStart<Item>(
  items: readonly Item[],
  start: number,
  getStart: (item: Item) => number,
): number {
  let low = 0;
  let high = items.length;

  while (low < high) {
    const middle = Math.floor((low + high) / 2);
    if (getStart(items[middle]) < start) {
      low = middle + 1;
    } else {
      high = middle;
    }
  }

  return low;
}

import type {
  ArcgisParquetColumnIndex,
  ArcgisParquetOffsetIndex,
  ArcgisParquetPageIndexSource,
  ArcgisParquetPageIndexTarget,
  ArcgisParquetPageStatistic,
} from "./arcgisPageIndexes";
import type { ParquetByteCoverage } from "../../../parquet/ParquetByteCoverage";
import type {
  ByteRange,
  ColumnLayout,
  FileLayout,
  RowGroupLayout,
} from "../../../parquet/fileLayout";

export interface FileStructurePage {
  pageIndex: number;
  rowStart: number;
  rowEnd: number;
  byteRange: ByteRange;
  compressedPageSize: number;
  statistic: ArcgisParquetPageStatistic;
}

export interface FileStructureGap {
  byteRange: ByteRange;
}

export type ColumnDetailState =
  | { type: "idle" }
  | { type: "loading" }
  | { type: "unavailable" }
  | { type: "failed"; message: string }
  | {
      type: "ready";
      pages: FileStructurePage[];
      gaps: FileStructureGap[];
    };

export interface FileStructureSnapshot {
  layout: FileLayout;
  coverage: ParquetByteCoverage;
}

export interface FileStructureState {
  selectedRowGroupIndex: number | null;
  expandedColumnIds: ReadonlySet<string>;
  detailColumnId: string | null;
  details: ReadonlyMap<string, ColumnDetailState>;
}

export class ParquetFileStructureStore {
  private selectedRowGroupIndex: number | null = null;
  private expandedColumnIds = new Set<string>();
  private detailColumnId: string | null = null;
  private details = new Map<string, ColumnDetailState>();
  private listeners = new Set<() => void>();
  private generation = 0;
  private state = this.createState();

  constructor(
    readonly snapshot: FileStructureSnapshot,
    private readonly source: ArcgisParquetPageIndexSource,
  ) {}

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getState = (): FileStructureState => this.state;

  close(): void {
    this.generation += 1;
    this.listeners.clear();
  }

  selectRowGroup(index: number): void {
    const openingRowGroup = this.selectedRowGroupIndex !== index;
    this.selectedRowGroupIndex = openingRowGroup ? index : null;
    this.expandedColumnIds.clear();
    this.detailColumnId = null;
    const defaultColumn = openingRowGroup
      ? this.defaultColumn(this.rowGroup(index))
      : undefined;
    const shouldLoad = defaultColumn
      ? this.selectColumn(defaultColumn)
      : false;
    this.publish();
    if (defaultColumn && shouldLoad) {
      void this.loadColumn(defaultColumn);
    }
  }

  toggleColumn(column: ColumnLayout): void {
    if (this.expandedColumnIds.has(column.id)) {
      this.expandedColumnIds.delete(column.id);
      this.detailColumnId = null;
      this.publish();
      return;
    }
    const shouldLoad = this.selectColumn(column);
    this.publish();
    if (shouldLoad) {
      void this.loadColumn(column);
    }
  }

  rowGroup(index: number): RowGroupLayout | undefined {
    return this.snapshot.layout.rowGroups.find((rowGroup) => rowGroup.index === index);
  }

  private selectColumn(column: ColumnLayout): boolean {
    this.expandedColumnIds = new Set([column.id]);
    const existingDetail = this.details.get(column.id);
    const shouldLoad = existingDetail === undefined;
    if (shouldLoad) {
      this.details.set(column.id, { type: "loading" });
    } else if (existingDetail.type !== "loading") {
      this.detailColumnId = column.id;
    }
    return shouldLoad;
  }

  private defaultColumn(
    rowGroup: RowGroupLayout | undefined,
  ): ColumnLayout | undefined {
    let selectedColumn = rowGroup?.columns[0];
    let selectedLoadedByteLength = selectedColumn
      ? this.snapshot.coverage.coveredByteLength(selectedColumn.byteRange)
      : 0;

    for (const column of rowGroup?.columns.slice(1) ?? []) {
      const loadedByteLength = this.snapshot.coverage.coveredByteLength(
        column.byteRange,
      );
      if (loadedByteLength > selectedLoadedByteLength) {
        selectedColumn = column;
        selectedLoadedByteLength = loadedByteLength;
      }
    }

    return selectedColumn;
  }

  private async loadColumn(column: ColumnLayout): Promise<void> {
    const generation = this.generation;
    const target: ArcgisParquetPageIndexTarget = {
      fileId: this.snapshot.layout.fileId,
      rowGroupIndex: column.rowGroupIndex,
      columnIndex: column.columnIndex,
    };

    try {
      const [columnIndex, offsetIndex] = await Promise.all([
        this.source.getColumnIndex(target),
        this.source.getOffsetIndex(target),
      ]);
      if (generation !== this.generation) {
        return;
      }

      this.markIndexRangesLoaded(column);
      if (!offsetIndex) {
        this.details.set(column.id, { type: "unavailable" });
      } else {
        this.details.set(column.id, {
          type: "ready",
          pages: mergePages(columnIndex, offsetIndex),
          gaps: deriveGaps(column.byteRange, offsetIndex),
        });
      }
      if (this.expandedColumnIds.has(column.id)) {
        this.detailColumnId = column.id;
      }
    } catch (error) {
      if (generation !== this.generation) {
        return;
      }
      this.details.set(column.id, {
        type: "failed",
        message: error instanceof Error ? error.message : String(error),
      });
      if (this.expandedColumnIds.has(column.id)) {
        this.detailColumnId = column.id;
      }
    }
    this.publish();
  }

  private markIndexRangesLoaded(column: ColumnLayout): void {
    const rowGroup = this.rowGroup(column.rowGroupIndex);
    const chunkIndex = rowGroup?.columns.findIndex(
      (candidate) => candidate.columnIndex === column.columnIndex,
    );
    if (chunkIndex === undefined || chunkIndex < 0) {
      return;
    }
    const pageIndexes = this.snapshot.layout.pageIndexes.filter(
      (index) =>
        index.rowGroupIndex === column.rowGroupIndex &&
        index.fieldName === column.fieldName,
    );
    for (const index of pageIndexes) {
      this.snapshot.coverage.add(index.byteRange);
    }
  }

  private publish(): void {
    this.state = this.createState();
    for (const listener of this.listeners) {
      listener();
    }
  }

  private createState(): FileStructureState {
    return {
      selectedRowGroupIndex: this.selectedRowGroupIndex,
      expandedColumnIds: new Set(this.expandedColumnIds),
      detailColumnId: this.detailColumnId,
      details: new Map(this.details),
    };
  }
}

function mergePages(
  columnIndex: ArcgisParquetColumnIndex | null,
  offsetIndex: ArcgisParquetOffsetIndex,
): FileStructurePage[] {
  if (columnIndex && columnIndex.pages.length !== offsetIndex.pages.length) {
    throw new Error("Column and offset indexes contain different page counts.");
  }

  return offsetIndex.pages.map((location, index) => ({
    pageIndex: location.pageIndex,
    rowStart: location.rowStart,
    rowEnd: location.rowEnd,
    byteRange: { start: location.byteStart, end: location.byteEnd },
    compressedPageSize: location.compressedPageSize,
    statistic: columnIndex?.pages[index] ?? { type: "unknown" },
  }));
}

function deriveGaps(
  columnRange: ByteRange,
  offsetIndex: ArcgisParquetOffsetIndex,
): FileStructureGap[] {
  const pages = [...offsetIndex.pages].sort(
    (first, second) => first.byteStart - second.byteStart,
  );
  const gaps: FileStructureGap[] = [];
  let start = columnRange.start;
  for (const page of pages) {
    if (start < page.byteStart) {
      gaps.push({ byteRange: { start, end: page.byteStart } });
    }
    start = Math.max(start, page.byteEnd);
  }
  if (start < columnRange.end) {
    gaps.push({ byteRange: { start, end: columnRange.end } });
  }
  return gaps;
}

import type {
  ParquetColumnIndex,
  ParquetOffsetIndex,
  ParquetPageIndexSource,
  ParquetPageIndexTarget,
  ParquetPageStatistic,
} from "./parquetPageIndexes";
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
  statistic: ParquetPageStatistic;
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

/**
 * Owns inspector selection state and asynchronously loaded Parquet page-index details.
 * It provides one external-store boundary so the dialog can react to navigation and loading without duplicating request state.
 */
export class ParquetFileStructureStore {
  private _selectedRowGroupIndex: number | null = null;
  private _expandedColumnIds = new Set<string>();
  private _detailColumnId: string | null = null;
  private _details = new Map<string, ColumnDetailState>();
  private _listeners = new Set<() => void>();
  private _generation = 0;
  private _state = this._createState();

  constructor(
    readonly snapshot: FileStructureSnapshot,
    private readonly _source: ParquetPageIndexSource,
  ) {}

  subscribe = (listener: () => void): (() => void) => {
    this._listeners.add(listener);
    return () => this._listeners.delete(listener);
  };

  getState = (): FileStructureState => this._state;

  close(): void {
    this._generation += 1;
    this._listeners.clear();
  }

  selectRowGroup(index: number): void {
    const openingRowGroup = this._selectedRowGroupIndex !== index;
    this._selectedRowGroupIndex = openingRowGroup ? index : null;
    this._expandedColumnIds.clear();
    this._detailColumnId = null;
    const defaultColumn = openingRowGroup
      ? this._defaultColumn(this.rowGroup(index))
      : undefined;
    const shouldLoad = defaultColumn
      ? this._selectColumn(defaultColumn)
      : false;
    this._publish();
    if (defaultColumn && shouldLoad) {
      void this._loadColumn(defaultColumn);
    }
  }

  toggleColumn(column: ColumnLayout): void {
    if (this._expandedColumnIds.has(column.id)) {
      this._expandedColumnIds.delete(column.id);
      this._detailColumnId = null;
      this._publish();
      return;
    }
    const shouldLoad = this._selectColumn(column);
    this._publish();
    if (shouldLoad) {
      void this._loadColumn(column);
    }
  }

  rowGroup(index: number): RowGroupLayout | undefined {
    return this.snapshot.layout.rowGroups.find((rowGroup) => rowGroup.index === index);
  }

  private _selectColumn(column: ColumnLayout): boolean {
    this._expandedColumnIds = new Set([column.id]);
    const existingDetail = this._details.get(column.id);
    const shouldLoad = existingDetail === undefined;
    if (shouldLoad) {
      this._details.set(column.id, { type: "loading" });
    } else if (existingDetail.type !== "loading") {
      this._detailColumnId = column.id;
    }
    return shouldLoad;
  }

  private _defaultColumn(
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

  private async _loadColumn(column: ColumnLayout): Promise<void> {
    const generation = this._generation;
    const target: ParquetPageIndexTarget = {
      fileId: this.snapshot.layout.fileId,
      rowGroupIndex: column.rowGroupIndex,
      columnIndex: column.columnIndex,
    };

    try {
      const [columnIndex, offsetIndex] = await Promise.all([
        this._source.getColumnIndex(target),
        this._source.getOffsetIndex(target),
      ]);
      if (generation !== this._generation) {
        return;
      }

      this._markIndexRangesLoaded(column);
      if (!offsetIndex) {
        this._details.set(column.id, { type: "unavailable" });
      } else {
        this._details.set(column.id, {
          type: "ready",
          pages: mergePages(columnIndex, offsetIndex),
          gaps: deriveGaps(column.byteRange, offsetIndex),
        });
      }
      if (this._expandedColumnIds.has(column.id)) {
        this._detailColumnId = column.id;
      }
    } catch (error) {
      if (generation !== this._generation) {
        return;
      }
      this._details.set(column.id, {
        type: "failed",
        message: error instanceof Error ? error.message : String(error),
      });
      if (this._expandedColumnIds.has(column.id)) {
        this._detailColumnId = column.id;
      }
    }
    this._publish();
  }

  private _markIndexRangesLoaded(column: ColumnLayout): void {
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

  private _publish(): void {
    this._state = this._createState();
    for (const listener of this._listeners) {
      listener();
    }
  }

  private _createState(): FileStructureState {
    return {
      selectedRowGroupIndex: this._selectedRowGroupIndex,
      expandedColumnIds: new Set(this._expandedColumnIds),
      detailColumnId: this._detailColumnId,
      details: new Map(this._details),
    };
  }
}

function mergePages(
  columnIndex: ParquetColumnIndex | null,
  offsetIndex: ParquetOffsetIndex,
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
  offsetIndex: ParquetOffsetIndex,
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

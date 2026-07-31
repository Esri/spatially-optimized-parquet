/**
 * `PageReader` resolves metadata paths to physical Parquet columns.
 * XZ statistics reject unrelated row groups and pages.
 * Hyparquet decodes selected data pages with any required dictionary bytes.
 * `PageReader` maps decoded values to row IDs.
 */
import {
  readColumnIndex,
  readOffsetIndex,
  type ColumnChunk,
  type ColumnMetaData,
  type FileMetaData,
  type SchemaTree,
} from "hyparquet";
import { compressors } from "hyparquet-compressors";
import { readColumn } from "hyparquet/src/column.js";
import { DEFAULT_PARSERS } from "hyparquet/src/convert.js";
import { getSchemaPath } from "hyparquet/src/schema.js";

import type { XZRange } from "./xz";
import type { RangeReadable } from "./RangeReader";

/**
 * `PhysicalColumn` identifies one leaf column chunk and its local and global row offsets.
 */
export interface PhysicalColumn {
  path: string[];
  rowGroupIndex: number;
  rowGroupStart: number;
  rowGroupRows: number;
  chunk: ColumnChunk;
  metadata: ColumnMetaData;
  schemaPath: SchemaTree[];
}

export interface IndexedPage {
  index: number;
  rowStart: number;
  rowEnd: number;
  byteStart: number;
  byteEnd: number;
  minimum?: unknown;
  maximum?: unknown;
}

export interface LeafPage<T> {
  rowStart: number;
  rowEnd: number;
  values: T[];
}

/**
 * `PageReader` owns immutable Parquet metadata and the start row for each row group.
 * `PageReader` selects pages through XZ statistics or row IDs.
 * Hyparquet decodes the selected column bytes.
 */
export class PageReader {
  private readonly _rowGroupStarts: number[];

  /** Calculate each row group's start row for global feature IDs. */
  constructor(
    private readonly _reader: RangeReadable,
    private readonly _metadata: FileMetaData,
  ) {
    let rowStart = 0;
    this._rowGroupStarts = _metadata.row_groups.map((rowGroup) => {
      const currentStart = rowStart;
      rowStart += Number(rowGroup.num_rows);
      return currentStart;
    });
  }

  /**
   * Reject row groups whose XZ statistics do not overlap the query ranges.
   * Keep a row group if its statistics are absent.
   */
  getColumnsMatchingXZ(
    path: readonly string[],
    ranges: XZRange[],
  ): PhysicalColumn[] {
    return this._metadata.row_groups.flatMap((_, rowGroupIndex) => {
      const column = this.getPhysicalColumn(rowGroupIndex, path);
      const statistics = column.metadata.statistics;
      const minimum = statistics?.min_value ?? statistics?.min;
      const maximum = statistics?.max_value ?? statistics?.max;

      return minimum === undefined ||
        maximum === undefined ||
        rangesOverlap(Number(minimum), Number(maximum), ranges)
        ? [column]
        : [];
    });
  }

  /**
   * Resolve an absolute metadata path to one physical leaf column in a row group.
   */
  getPhysicalColumn(
    rowGroupIndex: number,
    path: readonly string[],
  ): PhysicalColumn {
    const rowGroup = this._metadata.row_groups[rowGroupIndex];
    if (!rowGroup) {
      throw new Error(`Parquet row group ${rowGroupIndex} was not found.`);
    }

    const fieldName = path.join(".");
    const chunk = rowGroup.columns.find(
      ({ meta_data: column }) =>
        column?.path_in_schema.join(".") === fieldName,
    );
    if (!chunk?.meta_data) {
      throw new Error(
        `Parquet column ${fieldName} was not found in row group ${rowGroupIndex}.`,
      );
    }

    return {
      path: [...path],
      rowGroupIndex,
      rowGroupStart: this._rowGroupStarts[rowGroupIndex],
      rowGroupRows: Number(rowGroup.num_rows),
      chunk,
      metadata: chunk.meta_data,
      schemaPath: getSchemaPath(this._metadata.schema, [...path]),
    };
  }

  /**
   * Use Parquet `ColumnIndex` bounds to reject pages outside all XZ ranges.
   * Follow the Page Index rule in `spec/display-optimization.md`.
   */
  async selectPagesByXZ(
    column: PhysicalColumn,
    ranges: XZRange[],
    signal?: AbortSignal,
  ): Promise<IndexedPage[]> {
    const pages = await this._readPageIndex(column, signal);
    return pages.filter(
      ({ minimum, maximum }) =>
        minimum === undefined ||
        maximum === undefined ||
        rangesOverlap(Number(minimum), Number(maximum), ranges),
    );
  }

  /**
   * Select pages that contain at least one requested row ID.
   * Keep `rowIds` in low-to-high order from the XZ scan.
   */
  async selectPagesByRowId(
    column: PhysicalColumn,
    rowIds: number[],
    signal?: AbortSignal,
  ): Promise<IndexedPage[]> {
    if (rowIds.length === 0) {
      return [];
    }

    const pages = await this._readPageIndex(column, signal);
    const selectedPages: IndexedPage[] = [];
    let rowIndex = 0;

    for (const page of pages) {
      // Advance once through sorted row IDs.
      while (rowIndex < rowIds.length && rowIds[rowIndex] < page.rowStart) {
        rowIndex += 1;
      }
      if (rowIndex >= rowIds.length) {
        break;
      }
      if (rowIds[rowIndex] < page.rowEnd) {
        selectedPages.push(page);
      }
    }

    return selectedPages;
  }

  /**
   * Decode only the selected data pages.
   * Prefix dictionary bytes before decode when the column uses a dictionary page.
   */
  async readLeafPages<T>(
    column: PhysicalColumn,
    pages: IndexedPage[],
    {
      signal,
      binary = false,
    }: {
      signal?: AbortSignal;
      binary?: boolean;
    } = {},
  ): Promise<LeafPage<T>[]> {
    const dictionary = await this._readDictionary(column, signal);
    const output: LeafPage<T>[] = [];

    for (const page of pages) {
      signal?.throwIfAborted();
      const pageBuffer = await this._reader.read(
        page.byteStart,
        page.byteEnd,
        signal,
      );
      // Hyparquet accepts dictionary values when dictionary bytes precede the selected page.
      const buffer = dictionary
        ? concatenateBuffers(dictionary, pageBuffer)
        : pageBuffer;
      const decoded = readColumn(
        { view: new DataView(buffer), offset: 0 },
        {
          groupStart: page.rowStart,
          groupRows: column.rowGroupRows,
          selectStart: 0,
          selectEnd: page.rowEnd - page.rowStart,
        },
        {
          ...column.metadata,
          pathInSchema: column.path,
          element: column.schemaPath.at(-1)!.element,
          schemaPath: column.schemaPath,
          parsers: DEFAULT_PARSERS,
          compressors,
          utf8: binary ? false : undefined,
        },
      );

      output.push({
        rowStart: page.rowStart,
        rowEnd: page.rowEnd,
        values: flattenDecodedValues<T>(decoded.data),
      });
    }

    return output;
  }

  /**
   * Create one `IndexedPage` per page from both Parquet page indexes.
   * Require both indexes because `PageReader` does not scan full columns.
   */
  private async _readPageIndex(
    column: PhysicalColumn,
    signal?: AbortSignal,
  ): Promise<IndexedPage[]> {
    const {
      column_index_offset: columnIndexOffset,
      column_index_length: columnIndexLength,
      offset_index_offset: offsetIndexOffset,
      offset_index_length: offsetIndexLength,
    } = column.chunk;
    if (
      columnIndexOffset === undefined ||
      columnIndexLength === undefined ||
      offsetIndexOffset === undefined ||
      offsetIndexLength === undefined
    ) {
      throw new Error(
        `Parquet column ${column.path.join(".")} does not define page indexes.`,
      );
    }

    const [columnIndexBuffer, offsetIndexBuffer] = await Promise.all([
      this._reader.read(
        Number(columnIndexOffset),
        Number(columnIndexOffset) + columnIndexLength,
        signal,
      ),
      this._reader.read(
        Number(offsetIndexOffset),
        Number(offsetIndexOffset) + offsetIndexLength,
        signal,
      ),
    ]);
    const columnIndex = readColumnIndex(
      { view: new DataView(columnIndexBuffer), offset: 0 },
      column.schemaPath.at(-1)!.element,
    );
    const offsetIndex = readOffsetIndex({
      view: new DataView(offsetIndexBuffer),
      offset: 0,
    });

    return offsetIndex.page_locations.map((location, index, locations) => {
      const rowStart = Number(location.first_row_index);
      const rowEnd =
        index + 1 < locations.length
          ? Number(locations[index + 1].first_row_index)
          : column.rowGroupRows;
      const byteStart = Number(location.offset);

      return {
        index,
        rowStart,
        rowEnd,
        byteStart,
        byteEnd: byteStart + location.compressed_page_size,
        minimum: columnIndex.min_values[index],
        maximum: columnIndex.max_values[index],
      };
    });
  }

  /** Read the dictionary bytes before the first data page. */
  private async _readDictionary(
    column: PhysicalColumn,
    signal?: AbortSignal,
  ): Promise<ArrayBuffer | null> {
    const dictionaryOffset = column.metadata.dictionary_page_offset;
    if (dictionaryOffset === undefined) {
      return null;
    }

    return this._reader.read(
      Number(dictionaryOffset),
      Number(column.metadata.data_page_offset),
      signal,
    );
  }
}

/** Treat XZ statistics and query ranges as inclusive intervals. */
function rangesOverlap(
  minimum: number,
  maximum: number,
  ranges: XZRange[],
): boolean {
  return ranges.some(
    ({ start, end }) => minimum <= end && maximum >= start,
  );
}

/**
 * `concatenateBuffers` copies dictionary and data page bytes into a new buffer.
 * The copy preserves cached buffers.
 */
function concatenateBuffers(
  first: ArrayBuffer,
  second: ArrayBuffer,
): ArrayBuffer {
  const combined = new Uint8Array(first.byteLength + second.byteLength);
  combined.set(new Uint8Array(first));
  combined.set(new Uint8Array(second), first.byteLength);
  return combined.buffer;
}

/** Flatten Hyparquet chunks in physical value order. */
function flattenDecodedValues<T>(chunks: unknown[]): T[] {
  const values: T[] = [];

  for (const chunk of chunks) {
    if (Array.isArray(chunk) || ArrayBuffer.isView(chunk)) {
      const entries = chunk as ArrayLike<unknown>;
      for (let index = 0; index < entries.length; index += 1) {
        values.push(entries[index] as T);
      }
    } else {
      values.push(chunk as T);
    }
  }

  return values;
}

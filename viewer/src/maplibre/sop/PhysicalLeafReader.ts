// Copyright 2026 Esri
//
// Licensed under the Apache License Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

/**
 * `PhysicalLeafReader` reads sparse rows from one nested Parquet leaf.
 * Hyparquet 1.27.1's public API does not support selecting one nested physical leaf.
 * Its `columns` option selects top-level fields, so selecting `geolod` downloads every LOD
 * sibling. This narrow adapter keeps Hyparquet's page decoder while resolving one full
 * `path_in_schema` and fetching only pages that contain matched XZ rows.
 */
import {
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

import type { RangeReadable } from "./RangeReader";

interface PhysicalColumn {
  path: string[];
  rowGroupStart: number;
  rowGroupRows: number;
  chunk: ColumnChunk;
  metadata: ColumnMetaData;
  schemaPath: SchemaTree[];
}

interface IndexedPage {
  rowStart: number;
  rowEnd: number;
  byteStart: number;
  byteEnd: number;
}

export interface LeafRowReader {
  readRows(
    path: readonly string[],
    rowIds: number[],
    signal?: AbortSignal,
  ): Promise<Map<number, unknown>>;
}

/**
 * `PhysicalLeafReader` owns immutable Parquet metadata and row-group offsets.
 * It intentionally handles only the deep physical projection missing from Hyparquet's public API.
 */
export class PhysicalLeafReader implements LeafRowReader {
  private readonly _rowGroupStarts: number[];

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
   * Read only pages from one physical leaf that contain requested global row IDs.
   */
  async readRows(
    path: readonly string[],
    rowIds: number[],
    signal?: AbortSignal,
  ): Promise<Map<number, unknown>> {
    const valuesByRow = new Map<number, unknown>();
    let rowOffset = 0;

    for (
      let rowGroupIndex = 0;
      rowGroupIndex < this._metadata.row_groups.length;
      rowGroupIndex += 1
    ) {
      const rowGroupStart = this._rowGroupStarts[rowGroupIndex];
      const rowGroupRows = Number(
        this._metadata.row_groups[rowGroupIndex].num_rows,
      );
      const rowGroupEnd = rowGroupStart + rowGroupRows;
      while (rowOffset < rowIds.length && rowIds[rowOffset] < rowGroupStart) {
        rowOffset += 1;
      }
      const groupRowIds: number[] = [];
      while (rowOffset < rowIds.length && rowIds[rowOffset] < rowGroupEnd) {
        groupRowIds.push(rowIds[rowOffset] - rowGroupStart);
        rowOffset += 1;
      }
      if (groupRowIds.length === 0) {
        continue;
      }

      signal?.throwIfAborted();
      const column = this._getPhysicalColumn(rowGroupIndex, path);
      const pages = await this._selectPages(column, groupRowIds, signal);
      const requestedRows = new Set(groupRowIds);

      for (const page of pages) {
        const values = await this._readPage(column, page, signal);
        for (let index = 0; index < values.length; index += 1) {
          const localRowId = page.rowStart + index;
          if (requestedRows.has(localRowId)) {
            valuesByRow.set(rowGroupStart + localRowId, values[index]);
          }
        }
      }
    }

    return valuesByRow;
  }

  private _getPhysicalColumn(
    rowGroupIndex: number,
    path: readonly string[],
  ): PhysicalColumn {
    const rowGroup = this._metadata.row_groups[rowGroupIndex];
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
      rowGroupStart: this._rowGroupStarts[rowGroupIndex],
      rowGroupRows: Number(rowGroup.num_rows),
      chunk,
      metadata: chunk.meta_data,
      schemaPath: getSchemaPath(this._metadata.schema, [...path]),
    };
  }

  private async _selectPages(
    column: PhysicalColumn,
    rowIds: number[],
    signal?: AbortSignal,
  ): Promise<IndexedPage[]> {
    const pages = await this._readOffsetIndex(column, signal);
    const selectedPages: IndexedPage[] = [];
    let rowIndex = 0;

    for (const page of pages) {
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

  private async _readOffsetIndex(
    column: PhysicalColumn,
    signal?: AbortSignal,
  ): Promise<IndexedPage[]> {
    const {
      offset_index_offset: offsetIndexOffset,
      offset_index_length: offsetIndexLength,
    } = column.chunk;
    if (offsetIndexOffset === undefined || offsetIndexLength === undefined) {
      throw new Error(
        `Parquet column ${column.path.join(".")} does not define an offset index.`,
      );
    }

    const buffer = await this._reader.read(
      Number(offsetIndexOffset),
      Number(offsetIndexOffset) + offsetIndexLength,
      signal,
    );
    const index = readOffsetIndex({
      view: new DataView(buffer),
      offset: 0,
    });

    return index.page_locations.map((location, pageIndex, locations) => {
      const rowStart = Number(location.first_row_index);
      const rowEnd =
        pageIndex + 1 < locations.length
          ? Number(locations[pageIndex + 1].first_row_index)
          : column.rowGroupRows;
      const byteStart = Number(location.offset);

      return {
        rowStart,
        rowEnd,
        byteStart,
        byteEnd: byteStart + location.compressed_page_size,
      };
    });
  }

  private async _readPage(
    column: PhysicalColumn,
    page: IndexedPage,
    signal?: AbortSignal,
  ): Promise<unknown[]> {
    const [dictionary, pageBuffer] = await Promise.all([
      this._readDictionary(column, signal),
      this._reader.read(page.byteStart, page.byteEnd, signal),
    ]);
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
        utf8: false,
      },
    );

    return flattenDecodedValues(decoded.data);
  }

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

function concatenateBuffers(
  first: ArrayBuffer,
  second: ArrayBuffer,
): ArrayBuffer {
  const combined = new Uint8Array(first.byteLength + second.byteLength);
  combined.set(new Uint8Array(first));
  combined.set(new Uint8Array(second), first.byteLength);
  return combined.buffer;
}

function flattenDecodedValues(chunks: unknown[]): unknown[] {
  const values: unknown[] = [];

  for (const chunk of chunks) {
    if (Array.isArray(chunk) || ArrayBuffer.isView(chunk)) {
      const entries = chunk as ArrayLike<unknown>;
      for (let index = 0; index < entries.length; index += 1) {
        values.push(entries[index]);
      }
    } else {
      values.push(chunk);
    }
  }

  return values;
}

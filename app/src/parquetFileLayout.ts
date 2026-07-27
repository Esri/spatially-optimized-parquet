import {
  asyncBufferFromUrl,
  parquetMetadataAsync,
  type ColumnChunk,
  type FileMetaData,
} from "hyparquet";

export const syntheticSubchunkSize = 64 * 1024;
export const syntheticBlockSize = 1024 * 1024;

export interface ByteRange {
  start: number;
  end: number;
}

export interface SyntheticBlock {
  id: string;
  byteRange: ByteRange;
}

export interface ColumnLayout {
  fieldName: string;
  byteRange: ByteRange;
  blocks: SyntheticBlock[];
}

export interface RowGroupLayout {
  index: number;
  byteRange: ByteRange;
  columns: ColumnLayout[];
}

export interface PageIndexLayout {
  id: string;
  rowGroupIndex: number;
  fieldName: string;
  kind: "column" | "offset";
  byteRange: ByteRange;
}

export interface FileLayout {
  byteLength: number;
  footer: ByteRange;
  rowGroups: RowGroupLayout[];
  pageIndexes: PageIndexLayout[];
}

export interface RangeLifecycleObserver {
  startRange(range: ByteRange): string;
  completeRange(requestId: string): void;
  failRange(requestId: string): void;
}

export async function loadFileLayout(
  url: string,
  observer: RangeLifecycleObserver,
): Promise<FileLayout> {
  const file = await asyncBufferFromUrl({
    url,
    fetch: createTrackedFetch(observer),
  });
  const metadata = await parquetMetadataAsync(file);

  return deriveFileLayout(file.byteLength, metadata);
}

export function deriveFileLayout(byteLength: number, metadata: FileMetaData): FileLayout {
  const footer = deriveFooterRange(byteLength, metadata.metadata_length);
  const pageIndexes: PageIndexLayout[] = [];
  const rowGroups = metadata.row_groups.map((rowGroup, rowGroupIndex) => {
    const columns = rowGroup.columns.flatMap((column, columnIndex) => {
      const byteRange = deriveColumnRange(column, byteLength);
      if (!byteRange) {
        return [];
      }

      const fieldName = column.meta_data?.path_in_schema.join(".") ?? `C${columnIndex}`;
      pageIndexes.push(
        ...derivePageIndexes(column, byteLength, {
          fieldName,
          rowGroupIndex,
          columnIndex,
        }),
      );

      return [{
        fieldName,
        byteRange,
        blocks: splitRangeIntoBlocks(`rg${rowGroupIndex}-c${columnIndex}`, byteRange),
      }];
    });
    const byteRange = deriveEnvelope(columns.map((column) => column.byteRange));

    return {
      index: rowGroupIndex,
      byteRange: byteRange ?? { start: 0, end: 0 },
      columns,
    };
  });

  return { byteLength, footer, rowGroups, pageIndexes };
}

export function parseRangeHeader(value: string | null | undefined): ByteRange | null {
  const match = /^bytes=(\d+)-(\d+)$/.exec(value ?? "");
  if (!match) {
    return null;
  }

  const start = Number(match[1]);
  const inclusiveEnd = Number(match[2]);
  if (!Number.isSafeInteger(start) || !Number.isSafeInteger(inclusiveEnd) || inclusiveEnd < start) {
    return null;
  }

  return { start, end: inclusiveEnd + 1 };
}

export function rangesOverlap(first: ByteRange, second: ByteRange): boolean {
  return first.start < second.end && second.start < first.end;
}

function createTrackedFetch(observer: RangeLifecycleObserver): typeof fetch {
  return async (input, init) => {
    const requestHeaders = new Headers(
      init?.headers ?? (input instanceof Request ? input.headers : undefined),
    );
    const range = parseRangeHeader(requestHeaders.get("range"));
    const requestId = range ? observer.startRange(range) : null;

    try {
      const response = await globalThis.fetch(input, init);
      if (requestId) {
        if (response.ok) {
          observer.completeRange(requestId);
        } else {
          observer.failRange(requestId);
        }
      }

      return response;
    } catch (error) {
      if (requestId) {
        observer.failRange(requestId);
      }

      throw error;
    }
  };
}

function deriveFooterRange(byteLength: number, metadataLength: number): ByteRange {
  const start = byteLength - metadataLength - 8;
  if (!isValidRange({ start, end: byteLength }, byteLength)) {
    throw new Error("Parquet footer metadata range exceeds the file bounds.");
  }

  return { start, end: byteLength };
}

function deriveColumnRange(column: ColumnChunk, byteLength: number): ByteRange | null {
  const metadata = column.meta_data;
  if (!metadata) {
    return null;
  }

  const dictionaryPageOffset = toSafeNumber(metadata.dictionary_page_offset);
  const dataPageOffset = toSafeNumber(metadata.data_page_offset);
  const compressedSize = toSafeNumber(metadata.total_compressed_size);
  const start = dictionaryPageOffset ?? dataPageOffset;
  if (start === null || compressedSize === null) {
    return null;
  }

  const byteRange = { start, end: start + compressedSize };
  return isValidRange(byteRange, byteLength) ? byteRange : null;
}

function derivePageIndexes(
  column: ColumnChunk,
  byteLength: number,
  {
    fieldName,
    rowGroupIndex,
    columnIndex,
  }: {
    fieldName: string;
    rowGroupIndex: number;
    columnIndex: number;
  },
): PageIndexLayout[] {
  return [
    createPageIndexLayout("column", column.column_index_offset, column.column_index_length),
    createPageIndexLayout("offset", column.offset_index_offset, column.offset_index_length),
  ].flatMap((pageIndex) => {
    if (!pageIndex) {
      return [];
    }

    return [{
      ...pageIndex,
      id: `rg${rowGroupIndex}-c${columnIndex}-${pageIndex.kind}-index`,
      fieldName,
      rowGroupIndex,
    }];
  });

  function createPageIndexLayout(
    kind: PageIndexLayout["kind"],
    offset: bigint | undefined,
    length: number | undefined,
  ): Pick<PageIndexLayout, "kind" | "byteRange"> | null {
    const start = toSafeNumber(offset);
    if (start === null || length === undefined || !Number.isSafeInteger(length) || length <= 0) {
      return null;
    }

    const byteRange = { start, end: start + length };
    return isValidRange(byteRange, byteLength) ? { kind, byteRange } : null;
  }
}

function deriveEnvelope(ranges: ByteRange[]): ByteRange | null {
  if (ranges.length === 0) {
    return null;
  }

  return {
    start: Math.min(...ranges.map((range) => range.start)),
    end: Math.max(...ranges.map((range) => range.end)),
  };
}

function splitRangeIntoBlocks(prefix: string, byteRange: ByteRange): SyntheticBlock[] {
  const blocks: SyntheticBlock[] = [];

  for (let start = byteRange.start, index = 0; start < byteRange.end; start += syntheticBlockSize, index += 1) {
    blocks.push({
      id: `${prefix}-b${index}`,
      byteRange: { start, end: Math.min(start + syntheticBlockSize, byteRange.end) },
    });
  }

  return blocks;
}

function toSafeNumber(value: bigint | undefined): number | null {
  if (value === undefined) {
    return null;
  }

  const number = Number(value);
  return Number.isSafeInteger(number) && number >= 0 ? number : null;
}

function isValidRange(range: ByteRange, byteLength: number): boolean {
  return (
    Number.isSafeInteger(range.start) &&
    Number.isSafeInteger(range.end) &&
    range.start >= 0 &&
    range.end >= range.start &&
    range.end <= byteLength
  );
}

import type { ParquetNumericValue } from "../../diagnostics";

export interface ParquetPageIndexTarget {
  fileId: number;
  rowGroupIndex: number;
  columnIndex: number;
}

export type ParquetPageStatistic =
  | { type: "nullOnly" }
  | {
      type: "bounds";
      min: ParquetNumericValue;
      max: ParquetNumericValue;
    }
  | { type: "unknown" };

export interface ParquetColumnIndex {
  pages: ParquetPageStatistic[];
}

export interface ParquetPageLocation {
  pageIndex: number;
  rowStart: number;
  rowEnd: number;
  byteStart: number;
  byteEnd: number;
  compressedPageSize: number;
}

export interface ParquetOffsetIndex {
  pages: ParquetPageLocation[];
}

export interface ParquetPageIndexSource {
  getColumnIndex(
    target: ParquetPageIndexTarget,
  ): Promise<ParquetColumnIndex | null>;
  getOffsetIndex(
    target: ParquetPageIndexTarget,
  ): Promise<ParquetOffsetIndex | null>;
}

export function resolveParquetPageIndexSource(
  value: unknown,
): ParquetPageIndexSource {
  if (
    typeof value !== "object" ||
    value === null ||
    !("getColumnIndex" in value) ||
    typeof value.getColumnIndex !== "function" ||
    !("getOffsetIndex" in value) ||
    typeof value.getOffsetIndex !== "function"
  ) {
    throw new TypeError("Parquet source does not expose page-index methods.");
  }
  return value as ParquetPageIndexSource;
}

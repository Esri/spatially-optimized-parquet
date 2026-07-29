import type { ArcgisParquetNumericValue } from "./arcgisParquetDiagnostics";

export interface ArcgisParquetPageIndexTarget {
  fileId: string;
  rowGroupIndex: number;
  columnIndex: number;
}

export type ArcgisParquetPageStatistic =
  | { type: "nullOnly" }
  | {
      type: "bounds";
      min: ArcgisParquetNumericValue;
      max: ArcgisParquetNumericValue;
    }
  | { type: "unknown" };

export interface ArcgisParquetColumnIndex {
  pages: ArcgisParquetPageStatistic[];
}

export interface ArcgisParquetPageLocation {
  pageIndex: number;
  rowStart: number;
  rowEnd: number;
  byteStart: number;
  byteEnd: number;
  compressedPageSize: number;
}

export interface ArcgisParquetOffsetIndex {
  pages: ArcgisParquetPageLocation[];
}

export interface ArcgisParquetPageIndexSource {
  getColumnIndex(
    target: ArcgisParquetPageIndexTarget,
  ): Promise<ArcgisParquetColumnIndex | null>;
  getOffsetIndex(
    target: ArcgisParquetPageIndexTarget,
  ): Promise<ArcgisParquetOffsetIndex | null>;
}

export function resolveParquetPageIndexSource(
  value: unknown,
): ArcgisParquetPageIndexSource {
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
  return value as ArcgisParquetPageIndexSource;
}

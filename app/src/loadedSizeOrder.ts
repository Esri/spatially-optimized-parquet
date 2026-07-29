import type { ParquetByteCoverage } from "./parquetByteCoverage";
import type { ByteRange } from "./parquetFileLayout";

export function orderByLoadedByteLength<T extends { byteRange: ByteRange }>(
  items: readonly T[],
  coverage: ParquetByteCoverage,
  enabled: boolean,
): T[] {
  return orderByDescendingValue(
    items,
    (item) => coverage.coveredByteLength(item.byteRange),
    enabled,
  );
}

export function orderByDescendingValue<T>(
  items: readonly T[],
  value: (item: T) => number,
  enabled: boolean,
): T[] {
  if (!enabled) {
    return [...items];
  }

  return items
    .map((item, index) => ({
      item,
      index,
      value: value(item),
    }))
    .sort(
      (first, second) =>
        second.value - first.value ||
        first.index - second.index,
    )
    .map(({ item }) => item);
}

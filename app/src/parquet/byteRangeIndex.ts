import { type ByteRange, rangesOverlap } from "./fileLayout";

export interface IndexedByteRange<Value> {
  range: ByteRange;
  value: Value;
}

/**
 * Stores byte ranges in an order that supports efficient overlap queries.
 * It owns the search index so callers can find relevant file regions without scanning every entry.
 */
export class ByteRangeIndex<Value> {
  private readonly _entries: readonly IndexedByteRange<Value>[];
  private readonly _prefixMaximumEnds: readonly number[];

  constructor(entries: readonly IndexedByteRange<Value>[]) {
    this._entries = [...entries].sort(
      (first, second) =>
        first.range.start - second.range.start || first.range.end - second.range.end,
    );
    this._prefixMaximumEnds = this._entries.reduce<number[]>((maximumEnds, entry) => {
      const previousMaximum = maximumEnds.at(-1) ?? Number.NEGATIVE_INFINITY;
      maximumEnds.push(Math.max(previousMaximum, entry.range.end));
      return maximumEnds;
    }, []);
  }

  query(range: ByteRange): readonly IndexedByteRange<Value>[] {
    const firstCandidate = upperBound(
      this._prefixMaximumEnds,
      range.start,
      (end) => end,
    );
    const finalCandidate = lowerBound(this._entries, range.end, (entry) => entry.range.start);
    const result: IndexedByteRange<Value>[] = [];

    for (let index = firstCandidate; index < finalCandidate; index += 1) {
      const entry = this._entries[index];
      if (rangesOverlap(entry.range, range)) {
        result.push(entry);
      }
    }

    return result;
  }
}

function lowerBound<Item>(
  items: readonly Item[],
  value: number,
  getValue: (item: Item) => number,
): number {
  let lower = 0;
  let upper = items.length;

  while (lower < upper) {
    const middle = Math.floor((lower + upper) / 2);
    if (getValue(items[middle]) < value) {
      lower = middle + 1;
    } else {
      upper = middle;
    }
  }

  return lower;
}

function upperBound<Item>(
  items: readonly Item[],
  value: number,
  getValue: (item: Item) => number,
): number {
  let lower = 0;
  let upper = items.length;

  while (lower < upper) {
    const middle = Math.floor((lower + upper) / 2);
    if (getValue(items[middle]) <= value) {
      lower = middle + 1;
    } else {
      upper = middle;
    }
  }

  return lower;
}

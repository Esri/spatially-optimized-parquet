import { type ByteRange, rangesOverlap } from "./fileLayout";

export interface IndexedByteRange<Value> {
  range: ByteRange;
  value: Value;
}

export class ByteRangeIndex<Value> {
  private readonly entries: readonly IndexedByteRange<Value>[];
  private readonly prefixMaximumEnds: readonly number[];

  constructor(entries: readonly IndexedByteRange<Value>[]) {
    this.entries = [...entries].sort(
      (first, second) =>
        first.range.start - second.range.start || first.range.end - second.range.end,
    );
    this.prefixMaximumEnds = this.entries.reduce<number[]>((maximumEnds, entry) => {
      const previousMaximum = maximumEnds.at(-1) ?? Number.NEGATIVE_INFINITY;
      maximumEnds.push(Math.max(previousMaximum, entry.range.end));
      return maximumEnds;
    }, []);
  }

  query(range: ByteRange): readonly IndexedByteRange<Value>[] {
    const firstCandidate = upperBound(
      this.prefixMaximumEnds,
      range.start,
      (end) => end,
    );
    const finalCandidate = lowerBound(this.entries, range.end, (entry) => entry.range.start);
    const result: IndexedByteRange<Value>[] = [];

    for (let index = firstCandidate; index < finalCandidate; index += 1) {
      const entry = this.entries[index];
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

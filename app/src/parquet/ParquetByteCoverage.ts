import type { ByteRange } from "./fileLayout";
import { rangesOverlap } from "./fileLayout";

export type ByteCoverageState = "empty" | "partial" | "loaded";

export class ParquetByteCoverage {
  private _ranges: ByteRange[];

  constructor(ranges: readonly ByteRange[] = []) {
    this._ranges = [];
    for (const range of ranges) {
      this.add(range);
    }
  }

  clone(): ParquetByteCoverage {
    return new ParquetByteCoverage(this._ranges);
  }

  values(): readonly ByteRange[] {
    return this._ranges;
  }

  add(range: ByteRange): { addedByteLength: number; newRanges: ByteRange[] } {
    const newRanges = this.uncovered(range);
    let index = lowerBound(this._ranges, range.start);
    if (index > 0 && this._ranges[index - 1].end >= range.start) {
      index -= 1;
    }

    let merged = { ...range };
    let removedByteLength = 0;
    let count = 0;
    while (
      index + count < this._ranges.length &&
      this._ranges[index + count].start <= merged.end
    ) {
      const existing = this._ranges[index + count];
      merged = {
        start: Math.min(merged.start, existing.start),
        end: Math.max(merged.end, existing.end),
      };
      removedByteLength += existing.end - existing.start;
      count += 1;
    }
    this._ranges.splice(index, count, merged);

    return {
      addedByteLength: merged.end - merged.start - removedByteLength,
      newRanges,
    };
  }

  uncovered(range: ByteRange): ByteRange[] {
    const uncovered: ByteRange[] = [];
    let start = range.start;

    for (const covered of this._ranges) {
      if (covered.end <= start) {
        continue;
      }
      if (covered.start >= range.end) {
        break;
      }
      if (covered.start > start) {
        uncovered.push({ start, end: Math.min(covered.start, range.end) });
      }
      start = Math.max(start, covered.end);
      if (start >= range.end) {
        break;
      }
    }
    if (start < range.end) {
      uncovered.push({ start, end: range.end });
    }

    return uncovered;
  }

  coveredByteLength(range: ByteRange): number {
    return this._ranges.reduce((total, covered) => {
      if (!rangesOverlap(range, covered)) {
        return total;
      }
      return total + Math.min(range.end, covered.end) - Math.max(range.start, covered.start);
    }, 0);
  }

  coverageFraction(range: ByteRange): number {
    const byteLength = range.end - range.start;
    return byteLength === 0 ? 0 : this.coveredByteLength(range) / byteLength;
  }

  state(range: ByteRange): ByteCoverageState {
    const fraction = this.coverageFraction(range);
    return fraction === 0 ? "empty" : fraction === 1 ? "loaded" : "partial";
  }

  overlaps(range: ByteRange): boolean {
    const index = lowerBound(this._ranges, range.start);
    if (index > 0 && this._ranges[index - 1].end > range.start) {
      return true;
    }
    return index < this._ranges.length && rangesOverlap(range, this._ranges[index]);
  }
}

function lowerBound(ranges: readonly ByteRange[], start: number): number {
  let lower = 0;
  let upper = ranges.length;
  while (lower < upper) {
    const middle = Math.floor((lower + upper) / 2);
    if (ranges[middle].start < start) {
      lower = middle + 1;
    } else {
      upper = middle;
    }
  }
  return lower;
}

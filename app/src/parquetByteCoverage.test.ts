import { describe, expect, it } from "vitest";

import { ParquetByteCoverage } from "./parquetByteCoverage";

describe("ParquetByteCoverage", () => {
  it("merges overlaps while preserving clone isolation", () => {
    const coverage = new ParquetByteCoverage();
    coverage.add({ start: 0, end: 10 });
    coverage.add({ start: 5, end: 15 });
    const clone = coverage.clone();
    clone.add({ start: 20, end: 30 });

    expect(coverage.values()).toEqual([{ start: 0, end: 15 }]);
    expect(clone.values()).toEqual([
      { start: 0, end: 15 },
      { start: 20, end: 30 },
    ]);
    expect(coverage.state({ start: 0, end: 20 })).toBe("partial");
  });
});

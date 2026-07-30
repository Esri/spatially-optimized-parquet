import { describe, expect, it } from "vitest";

import { ByteRangeIndex } from "./byteRangeIndex";

describe("ByteRangeIndex", () => {
  it("returns every nested and duplicate-start interval that overlaps a range", () => {
    const index = new ByteRangeIndex([
      { range: { start: 0, end: 100 }, value: "outer" },
      { range: { start: 20, end: 30 }, value: "first" },
      { range: { start: 20, end: 40 }, value: "second" },
      { range: { start: 50, end: 60 }, value: "disjoint" },
    ]);

    expect(index.query({ start: 25, end: 55 }).map(({ value }) => value)).toEqual([
      "outer",
      "first",
      "second",
      "disjoint",
    ]);
    expect(index.query({ start: 100, end: 110 })).toEqual([]);
  });
});

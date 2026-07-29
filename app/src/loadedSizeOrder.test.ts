import { describe, expect, it } from "vitest";

import {
  orderByDescendingValue,
  orderByLoadedByteLength,
} from "./loadedSizeOrder";
import { ParquetByteCoverage } from "./parquetByteCoverage";

describe("orderByLoadedByteLength", () => {
  it("orders largest loaded ranges first and preserves source order for ties", () => {
    const coverage = new ParquetByteCoverage([
      { start: 0, end: 20 },
      { start: 100, end: 150 },
    ]);
    const items = [
      { id: "first", byteRange: { start: 0, end: 100 } },
      { id: "second", byteRange: { start: 100, end: 200 } },
      { id: "third", byteRange: { start: 200, end: 300 } },
      { id: "fourth", byteRange: { start: 300, end: 400 } },
    ];

    expect(
      orderByLoadedByteLength(items, coverage, true).map((item) => item.id),
    ).toEqual(["second", "first", "third", "fourth"]);
  });

  it("orders aggregate values without requiring a single byte range", () => {
    const items = [
      { id: "first", loadedByteLength: 10 },
      { id: "second", loadedByteLength: 30 },
      { id: "third", loadedByteLength: 10 },
    ];

    expect(
      orderByDescendingValue(
        items,
        (item) => item.loadedByteLength,
        true,
      ).map((item) => item.id),
    ).toEqual(["second", "first", "third"]);
  });
});

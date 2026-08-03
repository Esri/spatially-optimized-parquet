import { describe, expect, it } from "vitest";

import type { RowGroupBound } from "./rowGroupBounds";
import { calculateDefaultRowGroupExtent } from "./rowGroupExtent";

describe("calculateDefaultRowGroupExtent", () => {
  it("uses the selected extent unchanged below 200,000 dataset features", () => {
    const result = calculateDefaultRowGroupExtent([
      createRowGroup(0, 100_000, 0, 0, 10, 10),
    ]);

    expect(result).toEqual({
      extent: { xmin: 0, ymin: 0, xmax: 10, ymax: 10 },
      selectedFeatureCount: 100_000,
    });
  });

  it("shrinks each dimension by the square root feature ratio", () => {
    const result = calculateDefaultRowGroupExtent([
      createRowGroup(0, 800_000, 0, 0, 10, 10),
    ]);

    expect(result).toEqual({
      extent: { xmin: 2.5, ymin: 2.5, xmax: 7.5, ymax: 7.5 },
      selectedFeatureCount: 800_000,
    });
  });
});

function createRowGroup(
  rowGroupIndex: number,
  featureCount: number,
  xmin: number,
  ymin: number,
  xmax: number,
  ymax: number,
): RowGroupBound {
  return {
    fileId: 0,
    fileName: "file.parquet",
    rowGroupIndex,
    featureCount,
    xmin,
    ymin,
    xmax,
    ymax,
  };
}

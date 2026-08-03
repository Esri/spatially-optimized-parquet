import { describe, expect, it } from "vitest";

import type { ApproximateBound } from "./approximateBounds";
import { calculateDefaultXZExtent } from "./defaultExtent";

describe("calculateDefaultXZExtent", () => {
  it("uses the selected extent unchanged below 200,000 dataset features", () => {
    const result = calculateDefaultXZExtent([
      createRowGroup(0, 100_000, 0, 0, 10, 10),
    ]);

    expect(result).toEqual({
      extent: { xmin: 0, ymin: 0, xmax: 10, ymax: 10 },
      selectedFeatureCount: 100_000,
    });
  });

  it("shrinks each dimension by the square root feature ratio", () => {
    const result = calculateDefaultXZExtent([
      createRowGroup(0, 800_000, 0, 0, 10, 10),
    ]);

    expect(result).toEqual({
      extent: { xmin: 2.5, ymin: 2.5, xmax: 7.5, ymax: 7.5 },
      selectedFeatureCount: 800_000,
    });
  });

  it("excludes broad low-density row groups before counting and scaling", () => {
    const compactGroups = Array.from({ length: 4 }, (_, index) =>
      createRowGroup(index, 100_000, index, 30, index + 1, 31)
    );
    const worldGroup = createRowGroup(4, 100_000, -179, -80, 179, 80);
    const result = calculateDefaultXZExtent([...compactGroups, worldGroup]);

    expect(result).toEqual({
      extent: {
        xmin: 0.5857864376269049,
        ymin: 30.146446609406727,
        xmax: 3.414213562373095,
        ymax: 30.853553390593273,
      },
      selectedFeatureCount: 400_000,
    });
  });
});

function createRowGroup(
  rowGroupIndex: number,
  rowCount: number,
  xmin: number,
  ymin: number,
  xmax: number,
  ymax: number,
): ApproximateBound {
  return {
    fileId: 0,
    fileName: "file.parquet",
    rowGroupIndex,
    pageIndex: null,
    featureCount: rowCount,
    approximate: true,
    xmin,
    ymin,
    xmax,
    ymax,
  };
}

import { describe, expect, it } from "vitest";

import { decodeXZBounds } from "./decodeXZBounds";

const fullExtent = {
  xmin: 0,
  ymin: 0,
  xmax: 8,
  ymax: 8,
};

describe("decodeXZBounds", () => {
  it("unions the expanded endpoint cells for an XZ interval", () => {
    expect(
      decodeXZBounds(
        { min: "2", max: "5" },
        fullExtent,
        2,
      ),
    ).toEqual({
      xmin: 0,
      ymin: 0,
      xmax: 6,
      ymax: 6,
    });
  });

  it("supports bigint codes beyond JavaScript safe integer precision", () => {
    const maxLevel = 31;
    const code = 4n ** 31n;

    expect(
      decodeXZBounds(
        { min: code, max: code },
        fullExtent,
        maxLevel,
      ),
    ).toEqual(expect.objectContaining({
      xmin: expect.any(Number),
      ymin: expect.any(Number),
      xmax: expect.any(Number),
      ymax: expect.any(Number),
    }));
  });

  it("rejects reversed XZ intervals", () => {
    expect(() =>
      decodeXZBounds(
        { min: 5, max: 2 },
        fullExtent,
        2,
      )
    ).toThrow("xzBounds.min must not exceed xzBounds.max.");
  });
});

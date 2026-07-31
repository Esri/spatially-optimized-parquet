import { describe, expect, it } from "vitest";

import { parseXZDisplayMetadata } from "./metadata";

const transform = {
  scale: [1, 1, 1, 1],
  translate: [0, 0, 0, 0],
};

function createMetadata(
  code: unknown,
  columns: unknown[],
): Record<string, unknown> {
  return {
    type: "xz",
    encoding: "esriPBF",
    geometryType: "polygon",
    wkid: 4326,
    code,
    maxLevel: 12,
    fullExtent: {
      xmin: -180,
      ymin: -90,
      xmax: 180,
      ymax: 90,
    },
    levels: columns.map((column, level) => ({
      column,
      level,
      resolution: 2 ** -level,
      transform,
    })),
  };
}

describe("parseXZDisplayMetadata column paths", () => {
  it("normalizes string and multi-part array paths", () => {
    const metadata = parseXZDisplayMetadata(
      createMetadata(
        ["spatial", "xz"],
        ["geometry", ["geometry", "lod", "1"]],
      ),
    );

    expect(metadata.codePath).toEqual(["spatial", "xz"]);
    expect(metadata.levels.map(({ columnPath }) => columnPath)).toEqual([
      ["geometry"],
      ["geometry", "lod", "1"],
    ]);
  });

  it.each([
    ["", ["geometry"]],
    [[], ["geometry"]],
    [["spatial", ""], ["geometry"]],
    [["spatial", 1], ["geometry"]],
    ["xz", [""]],
    ["xz", [[]]],
    ["xz", [["geometry", ""]]],
    ["xz", [["geometry", 1]]],
  ])("rejects invalid code or LOD path %#", (code, columns) => {
    expect(() =>
      parseXZDisplayMetadata(createMetadata(code, columns)),
    ).toThrow(TypeError);
  });
});

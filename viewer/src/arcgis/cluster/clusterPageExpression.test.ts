import { describe, expect, it } from "vitest";

import { createClusterPageValueExpression } from "./clusterPageExpression";

describe("createClusterPageValueExpression", () => {
  it("binary-searches exact page starts independently for each file", () => {
    const expression = createClusterPageValueExpression(
      [
        { fileId: 0, pageStarts: [0, 10, 20], rowEnd: 30 },
        { fileId: 1, pageStarts: [0, 5], rowEnd: 10 },
      ],
      "OBJECTID",
      10,
    );

    expect(expression).toContain("Floor(objectId / 4294967296)");
    expect(expression).toContain("var pageStarts = [0,10,20]");
    expect(expression).toContain("var pageStarts = [0,5]");
    expect(expression).toContain("while (low <= high)");
    expect(expression).toContain("return high - Floor(high / 10) * 10");
  });
});

// Copyright 2026 Esri
//
// Licensed under the Apache License Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

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

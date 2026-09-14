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

import { ByteRangeIndex } from "./ByteRangeIndex";

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

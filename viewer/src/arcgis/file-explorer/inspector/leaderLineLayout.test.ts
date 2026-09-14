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

import {
  assignLeaderLineLanes,
  leaderLineRoutesIntersect,
} from "./leaderLineLayout";

describe("assignLeaderLineLanes", () => {
  it("separates overlapping spans while reusing lanes for disjoint spans", () => {
    const positioned = assignLeaderLineLanes([
      { id: "first", start: 5, end: 30 },
      { id: "second", start: 20, end: 40 },
      { id: "third", start: 45, end: 60 },
    ]);

    expect(positioned).toEqual([
      { id: "first", start: 5, end: 30, lane: 1 },
      { id: "second", start: 20, end: 40, lane: 0 },
      { id: "third", start: 45, end: 60, lane: 1 },
    ]);
  });

  it("places a line above another label stem that its horizontal route crosses", () => {
    const positioned = assignLeaderLineLanes([
      { id: "multiscale", start: 5, end: 30 },
      { id: "geometry", start: 20, end: 50 },
    ]);

    expect(positioned.find((span) => span.id === "geometry")?.lane)
      .toBeLessThan(
        positioned.find((span) => span.id === "multiscale")?.lane ?? 0,
      );
  });

  it("computes a zero-crossing route from measured starts and destinations", () => {
    const positioned = assignLeaderLineLanes([
      { id: "one", start: 8, end: 10 },
      { id: "two", start: 27, end: 30 },
      { id: "three", start: 28, end: 50 },
      { id: "four", start: 90, end: 70 },
      { id: "five", start: 94, end: 90 },
    ]);

    expect(
      leaderLineRoutesIntersect(
        positioned,
        positioned.map((span) => span.lane),
        0,
      ),
    ).toBe(false);
  });
});

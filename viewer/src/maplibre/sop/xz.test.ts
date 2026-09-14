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

import type { Bounds } from "./metadata";
import { getQueryXZRanges, type XZRange } from "./xz";

interface QueryRangeCase {
  name: string;
  queryExtent: Bounds;
  maxLevel: number;
  expected: XZRange[];
}

interface RectangularQueryRangeCase extends QueryRangeCase {
  fullExtent: Bounds;
}

const queryRangeCases: QueryRangeCase[] = [
  {
    name: "zero maximum level",
    queryExtent: { xmin: 0, ymin: 0, xmax: 10, ymax: 10 },
    maxLevel: 0,
    expected: [{ start: 0, end: 0 }],
  },
  {
    name: "full extent",
    queryExtent: { xmin: 0, ymin: 0, xmax: 10, ymax: 10 },
    maxLevel: 1,
    expected: [{ start: 0, end: 4 }],
  },
  {
    name: "lower-left quadrant",
    queryExtent: { xmin: 0, ymin: 0, xmax: 4.9, ymax: 4.9 },
    maxLevel: 1,
    expected: [{ start: 0, end: 1 }],
  },
  {
    name: "query touching the vertical midpoint",
    queryExtent: { xmin: 0, ymin: 0, xmax: 5, ymax: 4.9 },
    maxLevel: 1,
    expected: [{ start: 0, end: 1 }],
  },
  {
    name: "query crossing the vertical midpoint",
    queryExtent: { xmin: 0, ymin: 0, xmax: 5.000001, ymax: 4.9 },
    maxLevel: 1,
    expected: [{ start: 0, end: 2 }],
  },
  {
    name: "query touching the horizontal midpoint",
    queryExtent: { xmin: 0, ymin: 0, xmax: 4.9, ymax: 5 },
    maxLevel: 1,
    expected: [{ start: 0, end: 1 }],
  },
  {
    name: "query crossing the horizontal midpoint",
    queryExtent: { xmin: 0, ymin: 0, xmax: 4.9, ymax: 5.000001 },
    maxLevel: 1,
    expected: [
      { start: 0, end: 1 },
      { start: 3, end: 3 },
    ],
  },
  {
    name: "top-right quadrant",
    queryExtent: { xmin: 5, ymin: 5, xmax: 10, ymax: 10 },
    maxLevel: 2,
    expected: [
      { start: 0, end: 1 },
      { start: 5, end: 6 },
      { start: 9, end: 11 },
      { start: 13, end: 13 },
      { start: 15, end: 20 },
    ],
  },
  {
    name: "top-right query crossing the expanded-cell boundary",
    queryExtent: { xmin: 4.9, ymin: 5, xmax: 10, ymax: 10 },
    maxLevel: 2,
    expected: [
      { start: 0, end: 1 },
      { start: 4, end: 6 },
      { start: 9, end: 20 },
    ],
  },
  {
    name: "bottom-center strip",
    queryExtent: { xmin: 4.5, ymin: 0, xmax: 5.5, ymax: 0.25 },
    maxLevel: 2,
    expected: [
      { start: 0, end: 3 },
      { start: 6, end: 7 },
    ],
  },
  {
    name: "query exactly containing one expanded subtree",
    queryExtent: { xmin: 0, ymin: 0, xmax: 5, ymax: 5 },
    maxLevel: 3,
    expected: [{ start: 0, end: 21 }],
  },
  {
    name: "expanded subtree at maximum level 4",
    queryExtent: { xmin: 0, ymin: 0, xmax: 5, ymax: 5 },
    maxLevel: 4,
    expected: [{ start: 0, end: 85 }],
  },
  {
    name: "expanded subtree at maximum level 8",
    queryExtent: { xmin: 0, ymin: 0, xmax: 5, ymax: 5 },
    maxLevel: 8,
    expected: [{ start: 0, end: 21_845 }],
  },
  {
    name: "expanded subtree at maximum level 12",
    queryExtent: { xmin: 0, ymin: 0, xmax: 5, ymax: 5 },
    maxLevel: 12,
    expected: [{ start: 0, end: 5_592_405 }],
  },
  {
    name: "expanded subtree at maximum level 20",
    queryExtent: { xmin: 0, ymin: 0, xmax: 5, ymax: 5 },
    maxLevel: 20,
    expected: [{ start: 0, end: 366_503_875_925 }],
  },
  {
    name: "disjoint query",
    queryExtent: { xmin: -2, ymin: -2, xmax: -1, ymax: -1 },
    maxLevel: 2,
    expected: [],
  },
];

const rectangularQueryRangeCases: RectangularQueryRangeCase[] = [
  {
    name: "wide full extent with a narrow horizontal query",
    fullExtent: { xmin: 0, ymin: 0, xmax: 20, ymax: 10 },
    queryExtent: { xmin: 9, ymin: 0, xmax: 11, ymax: 0.25 },
    maxLevel: 2,
    expected: [
      { start: 0, end: 3 },
      { start: 6, end: 7 },
    ],
  },
  {
    name: "tall full extent with a narrow vertical query",
    fullExtent: { xmin: 0, ymin: 0, xmax: 10, ymax: 20 },
    queryExtent: { xmin: 0, ymin: 9, xmax: 0.25, ymax: 11 },
    maxLevel: 2,
    expected: [
      { start: 0, end: 2 },
      { start: 4, end: 4 },
      { start: 11, end: 12 },
    ],
  },
  {
    name: "shifted rectangular full extent",
    fullExtent: { xmin: 10.25, ymin: 20.25, xmax: 30.25, ymax: 28.25 },
    queryExtent: { xmin: 20.25, ymin: 24.25, xmax: 30.25, ymax: 28.25 },
    maxLevel: 2,
    expected: [
      { start: 0, end: 1 },
      { start: 5, end: 6 },
      { start: 9, end: 11 },
      { start: 13, end: 13 },
      { start: 15, end: 20 },
    ],
  },
  {
    name: "query width limiting the estimated level",
    fullExtent: { xmin: 0, ymin: 0, xmax: 20, ymax: 10 },
    queryExtent: { xmin: 0, ymin: 0, xmax: 10, ymax: 1 },
    maxLevel: 4,
    expected: [
      { start: 0, end: 12 },
      { start: 23, end: 33 },
    ],
  },
  {
    name: "query height limiting the estimated level",
    fullExtent: { xmin: 0, ymin: 0, xmax: 20, ymax: 10 },
    queryExtent: { xmin: 0, ymin: 0, xmax: 2, ymax: 5 },
    maxLevel: 4,
    expected: [
      { start: 0, end: 7 },
      { start: 13, end: 17 },
      { start: 44, end: 49 },
      { start: 55, end: 59 },
    ],
  },
  {
    name: "shifted rectangular extent with an asymmetric query",
    fullExtent: { xmin: 10.25, ymin: 20.25, xmax: 30.25, ymax: 28.25 },
    queryExtent: { xmin: 13.25, ymin: 21.25, xmax: 25.25, ymax: 22.25 },
    maxLevel: 3,
    expected: [
      { start: 0, end: 11 },
      { start: 22, end: 27 },
    ],
  },
];

describe("getQueryXZRanges", () => {
  it.each(queryRangeCases)(
    "returns the shared ranges for $name",
    ({ queryExtent, maxLevel, expected }) => {
      expect(
        getQueryXZRanges(
          { xmin: 0, ymin: 0, xmax: 10, ymax: 10 },
          queryExtent,
          maxLevel,
        ),
      ).toEqual(expected);
    },
  );

  it.each(rectangularQueryRangeCases)(
    "returns the shared ranges for $name",
    ({ fullExtent, queryExtent, maxLevel, expected }) => {
      expect(
        getQueryXZRanges(fullExtent, queryExtent, maxLevel),
      ).toEqual(expected);
    },
  );
});

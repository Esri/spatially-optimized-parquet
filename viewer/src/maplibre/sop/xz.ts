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

/**
 * Builds query ranges for XZ-order keys.
 *
 * This follows Böhm, Klump, and Kriegel, “XZ-Ordering: A Space-Filling Curve
 * for Objects with Spatial Extension” (1999). The paper enlarges each Z-order
 * element to twice its width and height toward the upper-right corner.
 *
 * Paper terms map to this implementation as follows:
 *
 * - A hierarchy cell is the paper's Z-order element.
 * - `expandExtent` applies the enlarged-element rule from definition 1.
 * - `maxLevel` is the paper's resolution `g`.
 * - `getCodeForLevel` implements one term from definition 2.
 * - `getElementCount` implements lemma 3.
 * - `getQueryXZRanges` implements query processing from section 4.2.
 *
 * Rust code generation lives in
 * `crates/spatial/src/optimized/clustering/xz.rs`. These ranges provide a
 * conservative filter, so `Query` still checks exact feature extents.
 */
import type { Bounds } from "./metadata";

/** Holds an inclusive interval of XZ codes selected by a query. */
export interface XZRange {
  start: number;
  end: number;
}

/** Tracks one hierarchy element while building query ranges. */
interface XZStackItem {
  extent: Bounds;
  codeSum: number;
  depth: number;
}

/**
 * Find the XZ-code ranges that may contain features intersecting a query.
 *
 * This implements section 4.2 of the paper. Start at `fullExtent` and walk
 * down the four Z-order quadrants:
 *
 * 1. Skip an enlarged element when it does not intersect `queryExtent`.
 * 2. Add a whole descendant interval when the query contains the element.
 * 3. Otherwise add the element's own code and continue into its children.
 *
 * The search stops four levels below the query's estimated level. This returns
 * broader ranges and fewer index predicates. Adjacent and overlapping ranges
 * are merged before returning.
 *
 * Keep `maxLevel` at 20 or less so every code remains an exact JavaScript
 * integer. It must match the depth used by the Rust writer.
 *
 * @example
 * ```ts
 * getQueryXZRanges(
 *   { xmin: 0, ymin: 0, xmax: 10, ymax: 10 },
 *   { xmin: 0, ymin: 0, xmax: 5, ymax: 5 },
 *   1,
 * );
 * // [{ start: 0, end: 1 }]
 * ```
 */
export function getQueryXZRanges(
  fullExtent: Bounds,
  queryExtent: Bounds,
  maxLevel: number,
): XZRange[] {
  const queryLevel = getExtentXZLevel(fullExtent, queryExtent, maxLevel);
  // Limit the search to four levels below the estimated query level to reduce extra cells.
  const maximumQueryDepth = Math.min(queryLevel + 4, maxLevel);
  const ranges: XZRange[] = [];
  const stack: XZStackItem[] = [
    { extent: fullExtent, codeSum: 0, depth: 0 },
  ];

  while (stack.length > 0) {
    const item = stack.pop()!;
    const expandedExtent = expandExtent(item.extent);
    if (!intersectsExtent(expandedExtent, queryExtent)) {
      continue;
    }

    if (
      item.depth === maximumQueryDepth ||
      containsExtent(queryExtent, expandedExtent)
    ) {
      // A query that covers the full subtree returns one inclusive XZ interval.
      let end = item.codeSum;
      for (let depth = item.depth; depth < maxLevel; depth += 1) {
        end += getCodeForLevel(3, depth, maxLevel);
      }
      ranges.push({ start: item.codeSum, end });
      continue;
    }

    ranges.push({ start: item.codeSum, end: item.codeSum });
    if (item.depth === maxLevel) {
      continue;
    }

    subdivideExtent(item.extent).forEach((extent, quadrant) => {
      stack.push({
        extent,
        codeSum:
          item.codeSum + getCodeForLevel(quadrant, item.depth, maxLevel),
        depth: item.depth + 1,
      });
    });
  }

  return mergeXZRanges(ranges);
}

/**
 * Calculate the finer candidate sequence length from lemma 1.
 *
 * The query traversal uses this size estimate only to choose a practical
 * stopping depth. It does not assign a stored feature code.
 */
export function getExtentXZLevel(
  fullExtent: Bounds,
  queryExtent: Bounds,
  maxLevel: number,
): number {
  const xLevel = Math.log2(
    (fullExtent.xmax - fullExtent.xmin) /
      (queryExtent.xmax - queryExtent.xmin),
  );
  const yLevel = Math.log2(
    (fullExtent.ymax - fullExtent.ymin) /
      (queryExtent.ymax - queryExtent.ymin),
  );

  return Math.min(Math.floor(Math.min(xLevel, yLevel)) + 1, maxLevel);
}

/** Test one code against sorted, disjoint XZ ranges with binary search. */
export function xzCodeMatches(code: number, ranges: XZRange[]): boolean {
  let left = 0;
  let right = ranges.length - 1;

  while (left <= right) {
    const middle = Math.floor((left + right) / 2);
    const range = ranges[middle];
    if (code < range.start) {
      right = middle - 1;
    } else if (code > range.end) {
      left = middle + 1;
    } else {
      return true;
    }
  }

  return false;
}

/** Sort XZ ranges and merge intervals that overlap or touch. */
export function mergeXZRanges(ranges: XZRange[]): XZRange[] {
  const sortedRanges = [...ranges].sort(
    (left, right) => left.start - right.start || left.end - right.end,
  );
  const mergedRanges: XZRange[] = [];

  for (const range of sortedRanges) {
    const current = mergedRanges.at(-1);
    if (current && current.end + 1 >= range.start) {
      current.end = Math.max(current.end, range.end);
    } else {
      mergedRanges.push({ ...range });
    }
  }

  return mergedRanges;
}

/** Add one quadrant term from definition 2's sequence-code formula. */
function getCodeForLevel(
  quadrant: number,
  depth: number,
  maxLevel: number,
): number {
  return quadrant * getElementCount(maxLevel, depth) + 1;
}

/** Count an element and all descendants through `maxLevel`, following lemma 3. */
function getElementCount(maxLevel: number, sequenceIndex: number): number {
  return (4 ** (maxLevel - sequenceIndex) - 1) / 3;
}

/** Apply the enlarged-element rule from definition 1. */
function expandExtent(extent: Bounds): Bounds {
  return {
    xmin: extent.xmin,
    ymin: extent.ymin,
    xmax: extent.xmax + (extent.xmax - extent.xmin),
    ymax: extent.ymax + (extent.ymax - extent.ymin),
  };
}

/** Split an element into quadrants 0, 1, 2, and 3 from figure 1. */
function subdivideExtent(extent: Bounds): Bounds[] {
  const xmid = (extent.xmin + extent.xmax) / 2;
  const ymid = (extent.ymin + extent.ymax) / 2;

  return [
    { xmin: extent.xmin, xmax: xmid, ymin: extent.ymin, ymax: ymid },
    { xmin: xmid, xmax: extent.xmax, ymin: extent.ymin, ymax: ymid },
    { xmin: extent.xmin, xmax: xmid, ymin: ymid, ymax: extent.ymax },
    { xmin: xmid, xmax: extent.xmax, ymin: ymid, ymax: extent.ymax },
  ];
}

/** Test whether two extents overlap with positive area. */
function intersectsExtent(first: Bounds, second: Bounds): boolean {
  return !(
    first.xmax <= second.xmin ||
    first.xmin >= second.xmax ||
    first.ymax <= second.ymin ||
    first.ymin >= second.ymax
  );
}

/** Test whether `container` completely contains `contained`. */
function containsExtent(container: Bounds, contained: Bounds): boolean {
  return (
    contained.xmin >= container.xmin &&
    contained.xmax <= container.xmax &&
    contained.ymin >= container.ymin &&
    contained.ymax <= container.ymax
  );
}

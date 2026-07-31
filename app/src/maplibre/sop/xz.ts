import type { Bounds } from "./metadata";

export interface XZRange {
  start: number;
  end: number;
}

interface XZStackItem {
  extent: Bounds;
  codeSum: number;
  depth: number;
}

export function getQueryXZRanges(
  fullExtent: Bounds,
  queryExtent: Bounds,
  maxLevel: number,
): XZRange[] {
  const queryLevel = getExtentXZLevel(fullExtent, queryExtent, maxLevel);
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

function getCodeForLevel(
  quadrant: number,
  depth: number,
  maxLevel: number,
): number {
  return quadrant * getElementCount(maxLevel, depth) + 1;
}

function getElementCount(maxLevel: number, sequenceIndex: number): number {
  return (4 ** (maxLevel - sequenceIndex) - 1) / 3;
}

function expandExtent(extent: Bounds): Bounds {
  return {
    xmin: extent.xmin,
    ymin: extent.ymin,
    xmax: extent.xmax + (extent.xmax - extent.xmin),
    ymax: extent.ymax + (extent.ymax - extent.ymin),
  };
}

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

function intersectsExtent(first: Bounds, second: Bounds): boolean {
  return !(
    first.xmax <= second.xmin ||
    first.xmin >= second.xmax ||
    first.ymax <= second.ymin ||
    first.ymin >= second.ymax
  );
}

function containsExtent(container: Bounds, contained: Bounds): boolean {
  return (
    contained.xmin >= container.xmin &&
    contained.xmax <= container.xmax &&
    contained.ymin >= container.ymin &&
    contained.ymax <= container.ymax
  );
}

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

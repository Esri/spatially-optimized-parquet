export interface XZExtent {
  xmin: number;
  ymin: number;
  xmax: number;
  ymax: number;
}

export type XZCode = bigint | number | string;

export interface XZCodeRange {
  min: XZCode;
  max: XZCode;
}

const maximumXZLevel = 31;

/**
 * Resolve an approximate extent from an inclusive XZ code range.
 * Expand endpoint cells because XZ codes cover their upper-right neighbors.
 */
export function decodeXZBounds(
  xzBounds: XZCodeRange,
  xzFullExtent: XZExtent,
  maxLevel: number,
): XZExtent {
  validateExtent(xzFullExtent);
  validateMaxLevel(maxLevel);

  const minimumCode = parseXZCode(xzBounds.min, "xzBounds.min");
  const maximumCode = parseXZCode(xzBounds.max, "xzBounds.max");
  if (minimumCode > maximumCode) {
    throw new RangeError("xzBounds.min must not exceed xzBounds.max.");
  }

  const largestCode = getLargestXZCode(maxLevel);
  if (maximumCode > largestCode) {
    throw new RangeError(
      `xzBounds.max exceeds the largest code for maxLevel ${maxLevel}.`,
    );
  }

  const minimumExtent = expandXZCell(
    decodeXZCell(minimumCode, xzFullExtent, maxLevel),
    xzFullExtent,
  );
  const maximumExtent = expandXZCell(
    decodeXZCell(maximumCode, xzFullExtent, maxLevel),
    xzFullExtent,
  );

  return {
    xmin: Math.min(minimumExtent.xmin, maximumExtent.xmin),
    ymin: Math.min(minimumExtent.ymin, maximumExtent.ymin),
    xmax: Math.max(minimumExtent.xmax, maximumExtent.xmax),
    ymax: Math.max(minimumExtent.ymax, maximumExtent.ymax),
  };
}

function decodeXZCell(
  code: bigint,
  fullExtent: XZExtent,
  maxLevel: number,
): XZExtent {
  let remainingCode = code;
  const cell = { ...fullExtent };

  for (
    let level = 0;
    level < maxLevel && remainingCode > 0n;
    level += 1
  ) {
    remainingCode -= 1n;
    const subtreeSize = getSubtreeSize(maxLevel - level);
    const quadrant = Number(remainingCode / subtreeSize);
    remainingCode %= subtreeSize;
    subdivideXZCell(cell, quadrant);
  }

  return cell;
}

function subdivideXZCell(cell: XZExtent, quadrant: number): void {
  const midpointX = (cell.xmin + cell.xmax) / 2;
  const midpointY = (cell.ymin + cell.ymax) / 2;

  switch (quadrant) {
    case 0:
      cell.xmax = midpointX;
      cell.ymax = midpointY;
      return;
    case 1:
      cell.xmin = midpointX;
      cell.ymax = midpointY;
      return;
    case 2:
      cell.xmax = midpointX;
      cell.ymin = midpointY;
      return;
    case 3:
      cell.xmin = midpointX;
      cell.ymin = midpointY;
      return;
    default:
      throw new RangeError(`Invalid XZ quadrant ${quadrant}.`);
  }
}

function expandXZCell(
  cell: XZExtent,
  fullExtent: XZExtent,
): XZExtent {
  return {
    xmin: cell.xmin,
    ymin: cell.ymin,
    xmax: Math.min(fullExtent.xmax, cell.xmax + cell.xmax - cell.xmin),
    ymax: Math.min(fullExtent.ymax, cell.ymax + cell.ymax - cell.ymin),
  };
}

function getSubtreeSize(levelCount: number): bigint {
  return (4n ** BigInt(levelCount) - 1n) / 3n;
}

function getLargestXZCode(maxLevel: number): bigint {
  return getSubtreeSize(maxLevel + 1) - 1n;
}

function parseXZCode(value: XZCode, name: string): bigint {
  if (typeof value === "number") {
    if (!Number.isSafeInteger(value) || value < 0) {
      throw new TypeError(`${name} must be a nonnegative safe integer.`);
    }
    return BigInt(value);
  }
  if (typeof value === "string" && !/^(?:0|[1-9]\d*)$/.test(value)) {
    throw new TypeError(`${name} must be a nonnegative integer.`);
  }

  const code = BigInt(value);
  if (code < 0n) {
    throw new TypeError(`${name} must be a nonnegative integer.`);
  }
  return code;
}

function validateExtent(extent: XZExtent): void {
  const values = [extent.xmin, extent.ymin, extent.xmax, extent.ymax];
  if (!values.every(Number.isFinite)) {
    throw new TypeError("xzFullExtent must contain finite coordinates.");
  }
  if (extent.xmin >= extent.xmax || extent.ymin >= extent.ymax) {
    throw new RangeError("xzFullExtent must have positive width and height.");
  }
}

function validateMaxLevel(maxLevel: number): void {
  if (
    !Number.isInteger(maxLevel) ||
    maxLevel < 0 ||
    maxLevel > maximumXZLevel
  ) {
    throw new RangeError(
      `maxLevel must be an integer between 0 and ${maximumXZLevel}.`,
    );
  }
}

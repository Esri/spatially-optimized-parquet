import type {
  BoundsExtent,
  RowGroupBound,
} from "./rowGroupBounds";

export interface DefaultRowGroupExtent {
  extent: BoundsExtent;
  selectedFeatureCount: number;
}

const earthRadiusMeters = 6_371_008.8;
const maximumDensityRatio = 10;
const minimumRetainedRatio = 0.25;
const targetFeatureCount = 200_000;

/**
 * Select a representative row-group extent and scale it toward 200,000 features.
 * Preserve aspect ratio by applying the square root of the target area ratio.
 */
export function calculateDefaultRowGroupExtent(
  bounds: readonly RowGroupBound[],
): DefaultRowGroupExtent | null {
  const focus = calculateRowGroupFocusExtent(bounds);
  if (!focus) {
    return null;
  }

  const datasetFeatureCount = sumFeatureCounts(bounds);
  if (
    datasetFeatureCount < targetFeatureCount ||
    focus.selectedFeatureCount <= targetFeatureCount
  ) {
    return focus;
  }

  const dimensionRatio = Math.sqrt(
    targetFeatureCount / focus.selectedFeatureCount,
  );
  return {
    extent: scaleExtent(focus.extent, dimensionRatio),
    selectedFeatureCount: focus.selectedFeatureCount,
  };
}

export function calculateRowGroupFocusExtent(
  bounds: readonly RowGroupBound[],
): DefaultRowGroupExtent | null {
  if (bounds.length === 0) {
    return null;
  }

  const selectedBounds = selectFocusBounds(bounds);
  const selectedFeatureCount = sumFeatureCounts(selectedBounds);
  const extent = combineBounds(selectedBounds);
  return { extent, selectedFeatureCount };
}

function selectFocusBounds(
  bounds: readonly RowGroupBound[],
): readonly RowGroupBound[] {
  const densityEntries = bounds.flatMap((bound) => {
    const area = calculateSphericalArea(bound);
    const density = bound.featureCount / area;
    return Number.isFinite(density) && density > 0
      ? [{ bound, logDensity: Math.log(density) }]
      : [];
  });
  if (densityEntries.length === 0) {
    return bounds;
  }

  const medianDensity = median(
    densityEntries.map(({ logDensity }) => logDensity),
  );
  const densityThreshold = medianDensity - Math.log(maximumDensityRatio);
  const retainedBounds = densityEntries
    .filter(({ logDensity }) => logDensity >= densityThreshold)
    .map(({ bound }) => bound);
  const minimumRetainedCount = Math.max(
    1,
    Math.ceil(bounds.length * minimumRetainedRatio),
  );

  return retainedBounds.length >= minimumRetainedCount
    ? retainedBounds
    : bounds;
}

function calculateSphericalArea({
  xmin,
  ymin,
  xmax,
  ymax,
}: RowGroupBound): number {
  const longitudeSpan = Math.min(Math.abs(xmax - xmin), 360);
  const longitudeRadians = degreesToRadians(longitudeSpan);
  const southRadians = degreesToRadians(Math.max(ymin, -90));
  const northRadians = degreesToRadians(Math.min(ymax, 90));

  return (
    earthRadiusMeters ** 2 *
    longitudeRadians *
    Math.abs(Math.sin(northRadians) - Math.sin(southRadians))
  );
}

function combineBounds(
  bounds: readonly RowGroupBound[],
): BoundsExtent {
  const longitudeExtent = combineLongitudes(bounds);
  let ymin = Number.POSITIVE_INFINITY;
  let ymax = Number.NEGATIVE_INFINITY;
  for (const bound of bounds) {
    ymin = Math.min(ymin, bound.ymin);
    ymax = Math.max(ymax, bound.ymax);
  }

  return { ...longitudeExtent, ymin, ymax };
}

function combineLongitudes(
  bounds: readonly RowGroupBound[],
): Pick<BoundsExtent, "xmin" | "xmax"> {
  const intervals = bounds
    .flatMap(({ xmin, xmax }) => splitLongitudeInterval(xmin, xmax))
    .sort(([leftStart], [rightStart]) => leftStart - rightStart);
  const mergedIntervals: Array<[number, number]> = [];

  for (const interval of intervals) {
    const previous = mergedIntervals.at(-1);
    if (previous && interval[0] <= previous[1]) {
      previous[1] = Math.max(previous[1], interval[1]);
    } else {
      mergedIntervals.push([...interval]);
    }
  }

  let largestGapStart = mergedIntervals.at(-1)?.[1] ?? mergedIntervals[0][1];
  let largestGapEnd = mergedIntervals[0][0] + 360;
  for (let index = 0; index < mergedIntervals.length; index += 1) {
    const currentEnd = mergedIntervals[index][1];
    const nextStart = index + 1 < mergedIntervals.length
      ? mergedIntervals[index + 1][0]
      : mergedIntervals[0][0] + 360;
    if (nextStart - currentEnd > largestGapEnd - largestGapStart) {
      largestGapStart = currentEnd;
      largestGapEnd = nextStart;
    }
  }

  const xmin = normalizeLongitude(largestGapEnd);
  const span = 360 - (largestGapEnd - largestGapStart);
  return { xmin, xmax: xmin + span };
}

function splitLongitudeInterval(
  xmin: number,
  xmax: number,
): Array<[number, number]> {
  const span = Math.min(Math.abs(xmax - xmin), 360);
  if (span >= 360) {
    return [[0, 360]];
  }

  const start = normalizeLongitude360(xmin);
  const end = start + span;
  return end <= 360
    ? [[start, end]]
    : [[start, 360], [0, end - 360]];
}

function scaleExtent(
  extent: BoundsExtent,
  dimensionRatio: number,
): BoundsExtent {
  const centerX = (extent.xmin + extent.xmax) / 2;
  const centerY = (extent.ymin + extent.ymax) / 2;
  const halfWidth = ((extent.xmax - extent.xmin) * dimensionRatio) / 2;
  const halfHeight = ((extent.ymax - extent.ymin) * dimensionRatio) / 2;

  return {
    xmin: centerX - halfWidth,
    ymin: centerY - halfHeight,
    xmax: centerX + halfWidth,
    ymax: centerY + halfHeight,
  };
}

function sumFeatureCounts(bounds: readonly RowGroupBound[]): number {
  return bounds.reduce((sum, { featureCount }) => sum + featureCount, 0);
}

function median(values: number[]): number {
  const sortedValues = [...values].sort((left, right) => left - right);
  const middleIndex = Math.floor(sortedValues.length / 2);
  return sortedValues.length % 2 === 0
    ? (sortedValues[middleIndex - 1] + sortedValues[middleIndex]) / 2
    : sortedValues[middleIndex];
}

function normalizeLongitude(longitude: number): number {
  const normalized = normalizeLongitude360(longitude);
  return normalized > 180 ? normalized - 360 : normalized;
}

function normalizeLongitude360(longitude: number): number {
  return ((longitude % 360) + 360) % 360;
}

function degreesToRadians(degrees: number): number {
  return (degrees * Math.PI) / 180;
}

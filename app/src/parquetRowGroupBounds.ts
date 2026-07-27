import { asyncBufferFromUrl, parquetMetadataAsync } from "hyparquet";

export interface RowGroupBounds {
  rowGroupIndex: number;
  rowCount: number;
  xmin: number;
  ymin: number;
  xmax: number;
  ymax: number;
}

export interface BoundsExtent {
  xmin: number;
  ymin: number;
  xmax: number;
  ymax: number;
}

const rowGroupBoundsByUrl = new Map<string, Promise<RowGroupBounds[]>>();
const earthRadiusMeters = 6_371_008.8;
const minimumDensityRatio = 10;
const minimumRetainedRatio = 0.25;

export async function loadParquetRowGroupBounds(url: string): Promise<RowGroupBounds[]> {
  const cachedBounds = rowGroupBoundsByUrl.get(url);
  if (cachedBounds) {
    return cachedBounds;
  }

  const boundsPromise = loadRowGroupBounds(url);
  rowGroupBoundsByUrl.set(url, boundsPromise);

  try {
    return await boundsPromise;
  } catch (error) {
    rowGroupBoundsByUrl.delete(url);
    throw error;
  }
}

async function loadRowGroupBounds(url: string): Promise<RowGroupBounds[]> {
  const file = await asyncBufferFromUrl({ url });
  const metadata = await parquetMetadataAsync(file);

  return metadata.row_groups.map((rowGroup, rowGroupIndex) => {
    const geometryMetadata = rowGroup.columns.find(
      ({ meta_data: columnMetadata }) =>
        columnMetadata?.path_in_schema.join(".") === "geometry",
    )?.meta_data;
    const bounds = geometryMetadata?.geospatial_statistics?.bbox;

    if (!bounds) {
      throw new Error(`Geometry geospatial bbox not found for row group ${rowGroupIndex}.`);
    }

    return {
      rowGroupIndex,
      rowCount: Number(rowGroup.num_rows),
      ...bounds,
    };
  });
}

export function calculateRowGroupFocusExtent(
  rowGroups: RowGroupBounds[],
): BoundsExtent | undefined {
  if (rowGroups.length === 0) {
    return undefined;
  }

  const densityEntries = rowGroups.flatMap((rowGroup) => {
    const area = calculateSphericalArea(rowGroup);
    const density = rowGroup.rowCount / area;
    return Number.isFinite(density) && density > 0
      ? [{ rowGroup, logDensity: Math.log(density) }]
      : [];
  });

  if (densityEntries.length === 0) {
    return combineBounds(rowGroups);
  }

  const medianDensity = median(densityEntries.map(({ logDensity }) => logDensity));
  const medianAbsoluteDeviation = median(
    densityEntries.map(({ logDensity }) => Math.abs(logDensity - medianDensity)),
  );
  const robustDeviation = 1.4826 * medianAbsoluteDeviation;
  const densityThreshold =
    medianDensity - Math.max(3 * robustDeviation, Math.log(minimumDensityRatio));
  const retainedGroups = densityEntries
    .filter(({ logDensity }) => logDensity >= densityThreshold)
    .map(({ rowGroup }) => rowGroup);
  const minimumRetainedCount = Math.max(
    1,
    Math.ceil(rowGroups.length * minimumRetainedRatio),
  );

  return combineBounds(
    retainedGroups.length >= minimumRetainedCount ? retainedGroups : rowGroups,
  );
}

function calculateSphericalArea({ xmin, ymin, xmax, ymax }: RowGroupBounds): number {
  const longitudeSpan = Math.min(normalizeLongitudeSpan(xmax - xmin), 360);
  const westRadians = degreesToRadians(longitudeSpan);
  const southRadians = degreesToRadians(Math.max(ymin, -90));
  const northRadians = degreesToRadians(Math.min(ymax, 90));

  return (
    earthRadiusMeters ** 2 *
    westRadians *
    Math.abs(Math.sin(northRadians) - Math.sin(southRadians))
  );
}

function combineBounds(rowGroups: RowGroupBounds[]): BoundsExtent {
  const longitudeExtent = combineLongitudes(rowGroups);

  return {
    ...longitudeExtent,
    ymin: Math.min(...rowGroups.map(({ ymin }) => ymin)),
    ymax: Math.max(...rowGroups.map(({ ymax }) => ymax)),
  };
}

function combineLongitudes(rowGroups: RowGroupBounds[]): Pick<BoundsExtent, "xmin" | "xmax"> {
  const intervals = rowGroups
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

  let largestGapStart = mergedIntervals[0][1];
  let largestGapEnd = mergedIntervals[0][0] + 360;
  for (let index = 0; index < mergedIntervals.length; index += 1) {
    const currentEnd = mergedIntervals[index][1];
    const nextStart =
      index + 1 < mergedIntervals.length
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

function splitLongitudeInterval(xmin: number, xmax: number): Array<[number, number]> {
  const span = normalizeLongitudeSpan(xmax - xmin);
  if (span >= 360) {
    return [[0, 360]];
  }

  const start = normalizeLongitude360(xmin);
  const end = start + span;
  return end <= 360
    ? [[start, end]]
    : [
        [start, 360],
        [0, end - 360],
      ];
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

function normalizeLongitudeSpan(longitudeSpan: number): number {
  return Math.min(Math.abs(longitudeSpan), 360);
}

function degreesToRadians(degrees: number): number {
  return (degrees * Math.PI) / 180;
}

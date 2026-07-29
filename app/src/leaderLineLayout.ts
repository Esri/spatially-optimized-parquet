export interface LeaderLineSpan {
  id: string;
  start: number;
  end: number;
}

export interface PositionedLeaderLineSpan extends LeaderLineSpan {
  lane: number;
}

export function assignLeaderLineLanes(
  spans: readonly LeaderLineSpan[],
  minimumGap = 0,
): PositionedLeaderLineSpan[] {
  if (spans.length === 0) {
    return [];
  }

  for (let laneCount = 1; laneCount <= spans.length; laneCount += 1) {
    const lanes = findBestLaneAssignment(spans, laneCount, minimumGap);
    if (lanes) {
      return spans.map((span, index) => ({
        ...span,
        lane: lanes[index],
      }));
    }
  }

  throw new Error("Unable to route leader lines without crossings.");
}

function findBestLaneAssignment(
  spans: readonly LeaderLineSpan[],
  laneCount: number,
  minimumGap: number,
): number[] | null {
  let bestLanes: number[] | null = null;
  let bestOrderCost = Number.POSITIVE_INFINITY;
  const lanes = new Array<number>(spans.length).fill(0);

  const visit = (spanIndex: number) => {
    if (spanIndex === spans.length) {
      if (new Set(lanes).size !== laneCount) {
        return;
      }
      if (leaderLineRoutesIntersect(spans, lanes, minimumGap)) {
        return;
      }

      const orderCost = lanes.reduce((cost, lane, index) => {
        const preferredLane = spans.length === 1
          ? 0
          : (index / (spans.length - 1)) * (laneCount - 1);
        return cost + Math.abs(lane - preferredLane);
      }, 0);
      if (orderCost < bestOrderCost) {
        bestOrderCost = orderCost;
        bestLanes = [...lanes];
      }
      return;
    }

    for (let lane = 0; lane < laneCount; lane += 1) {
      lanes[spanIndex] = lane;
      visit(spanIndex + 1);
    }
  };

  visit(0);
  return bestLanes;
}

export function leaderLineRoutesIntersect(
  spans: readonly LeaderLineSpan[],
  lanes: readonly number[],
  minimumGap: number,
): boolean {
  for (let firstIndex = 0; firstIndex < spans.length; firstIndex += 1) {
    const first = spans[firstIndex];
    const firstLeft = Math.min(first.start, first.end);
    const firstRight = Math.max(first.start, first.end);

    for (let secondIndex = 0; secondIndex < spans.length; secondIndex += 1) {
      if (firstIndex === secondIndex) {
        continue;
      }

      const second = spans[secondIndex];
      const secondLeft = Math.min(second.start, second.end);
      const secondRight = Math.max(second.start, second.end);

      if (
        lanes[firstIndex] === lanes[secondIndex] &&
        firstLeft <= secondRight + minimumGap &&
        secondLeft <= firstRight + minimumGap
      ) {
        return true;
      }

      if (
        isInsideHorizontalSpan(second.start, firstLeft, firstRight) &&
        lanes[firstIndex] <= lanes[secondIndex]
      ) {
        return true;
      }

      if (
        isInsideHorizontalSpan(second.end, firstLeft, firstRight) &&
        lanes[firstIndex] >= lanes[secondIndex]
      ) {
        return true;
      }
    }
  }

  return false;
}

function isInsideHorizontalSpan(
  position: number,
  left: number,
  right: number,
): boolean {
  return position > left && position < right;
}

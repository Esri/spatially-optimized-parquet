import type { ArcgisParquetRangeReadEvent } from "../../diagnostics";
import type { ByteRange } from "../../../parquet/fileLayout";

interface ActiveRequest {
  range: ByteRange;
}

export type RangeReadChange =
  | { type: "duplicate" }
  | { type: "unknown-file"; fileId: string }
  | {
      type: "start";
      requestKey: string;
      range: ByteRange;
      previousLatestRequestKey: string | null;
    }
  | {
      type: "finish";
      requestKey: string;
      range: ByteRange;
      phase: "complete" | "error";
      latestRequestKey: string | null;
      replacedLatestRequest: boolean;
    };

export class RangeReadLedger {
  private readonly activeRequests = new Map<string, ActiveRequest>();
  private readonly handledEventKeys = new Set<string>();
  private latestRequestKey: string | null = null;
  private completedRequestCount = 0;

  reset(): void {
    this.activeRequests.clear();
    this.handledEventKeys.clear();
    this.latestRequestKey = null;
    this.completedRequestCount = 0;
  }

  record(
    event: ArcgisParquetRangeReadEvent,
    acceptsFile: (fileId: string) => boolean,
  ): RangeReadChange {
    const requestKey = createRequestKey(event);
    const eventKey = `${requestKey}:${event.phase}`;
    if (this.handledEventKeys.has(eventKey)) {
      return { type: "duplicate" };
    }
    if (!acceptsFile(event.fileId)) {
      return { type: "unknown-file", fileId: event.fileId };
    }

    this.handledEventKeys.add(eventKey);
    if (event.phase === "start") {
      const previousLatestRequestKey = this.latestRequestKey;
      this.latestRequestKey = requestKey;
      this.activeRequests.set(requestKey, { range: event.range });
      return {
        type: "start",
        requestKey,
        range: event.range,
        previousLatestRequestKey,
      };
    }

    const replacedLatestRequest = this.latestRequestKey === requestKey;
    this.activeRequests.delete(requestKey);
    if (replacedLatestRequest) {
      this.latestRequestKey = this.activeRequests.keys().next().value ?? null;
    }
    if (event.phase === "complete") {
      this.completedRequestCount += 1;
    }
    return {
      type: "finish",
      requestKey,
      range: event.range,
      phase: event.phase,
      latestRequestKey: this.latestRequestKey,
      replacedLatestRequest,
    };
  }

  activeRequestEntries(): IterableIterator<[string, ActiveRequest]> {
    return this.activeRequests.entries();
  }

  currentLatestRequestKey(): string | null {
    return this.latestRequestKey;
  }

  currentCompletedRequestCount(): number {
    return this.completedRequestCount;
  }
}

function createRequestKey(
  event: Pick<ArcgisParquetRangeReadEvent, "fileId" | "requestId">,
): string {
  return `${encodeURIComponent(event.fileId)}:${event.requestId}`;
}

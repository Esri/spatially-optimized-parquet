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

/**
 * Tracks active and completed Parquet range requests while rejecting duplicate diagnostic events.
 * It owns request identity and ordering so projection updates can apply each lifecycle transition once.
 */
export class RangeReadLedger {
  private readonly _activeRequests = new Map<string, ActiveRequest>();
  private readonly _handledEventKeys = new Set<string>();
  private _latestRequestKey: string | null = null;
  private _completedRequestCount = 0;

  reset(): void {
    this._activeRequests.clear();
    this._handledEventKeys.clear();
    this._latestRequestKey = null;
    this._completedRequestCount = 0;
  }

  record(
    event: ArcgisParquetRangeReadEvent,
    acceptsFile: (fileId: string) => boolean,
  ): RangeReadChange {
    const requestKey = createRequestKey(event);
    const eventKey = `${requestKey}:${event.phase}`;
    if (this._handledEventKeys.has(eventKey)) {
      return { type: "duplicate" };
    }
    if (!acceptsFile(event.fileId)) {
      return { type: "unknown-file", fileId: event.fileId };
    }

    this._handledEventKeys.add(eventKey);
    if (event.phase === "start") {
      const previousLatestRequestKey = this._latestRequestKey;
      this._latestRequestKey = requestKey;
      this._activeRequests.set(requestKey, { range: event.range });
      return {
        type: "start",
        requestKey,
        range: event.range,
        previousLatestRequestKey,
      };
    }

    const replacedLatestRequest = this._latestRequestKey === requestKey;
    this._activeRequests.delete(requestKey);
    if (replacedLatestRequest) {
      this._latestRequestKey = this._activeRequests.keys().next().value ?? null;
    }
    if (event.phase === "complete") {
      this._completedRequestCount += 1;
    }
    return {
      type: "finish",
      requestKey,
      range: event.range,
      phase: event.phase,
      latestRequestKey: this._latestRequestKey,
      replacedLatestRequest,
    };
  }

  get activeRequestEntries(): IterableIterator<[string, ActiveRequest]> {
    return this._activeRequests.entries();
  }

  get latestRequestKey(): string | null {
    return this._latestRequestKey;
  }

  get completedRequestCount(): number {
    return this._completedRequestCount;
  }
}

function createRequestKey(
  event: Pick<ArcgisParquetRangeReadEvent, "fileId" | "requestId">,
): string {
  return `${encodeURIComponent(event.fileId)}:${event.requestId}`;
}

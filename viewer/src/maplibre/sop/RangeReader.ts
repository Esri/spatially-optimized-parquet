/**
 * `RangeReader` reads byte ranges from one remote Parquet file.
 * `RangeReader` validates HTTP partial responses and caches completed exact ranges.
 * Hyparquet uses its `AsyncBuffer` adapter for page-index predicate pushdown.
 */
import type { AsyncBuffer } from "hyparquet";

/** `RangeReadable` defines the byte source for metadata and page reads. */
export interface RangeReadable {
  read(start: number, end: number, signal?: AbortSignal): Promise<ArrayBuffer>;
  asAsyncBuffer(signal?: AbortSignal): AsyncBuffer;
  clear(): void;
}

/**
 * `RangeReader` owns the remote file state and a cache of completed ranges.
 * `RangeReader` validates each HTTP partial response.
 * `RangeReader` does not combine requests whose ranges overlap or requests that remain in progress.
 */
export class RangeReader implements RangeReadable {
  readonly byteLength: number;

  private readonly _completedRangeCache = new Map<string, ArrayBuffer>();

  constructor(
    private readonly _url: string,
    byteLength: number,
  ) {
    this.byteLength = byteLength;
  }

  /**
   * Read the half-open byte range `[start, end)` with an HTTP `Range` request.
   * Before cache insert, require status 206 and the exact byte count.
   * Do not parse the `Content-Range` header.
   */
  async read(
    start: number,
    end: number,
    signal?: AbortSignal,
  ): Promise<ArrayBuffer> {
    validateRange(start, end, this.byteLength);
    const cacheKey = `${start}:${end}`;
    const cached = this._completedRangeCache.get(cacheKey);
    if (cached) {
      return cached;
    }

    const response = await fetch(this._url, {
      headers: {
        Range: `bytes=${start}-${end - 1}`,
      },
      signal,
    });
    if (response.status !== 206) {
      throw new Error(
        `Parquet range request ${start}-${end - 1} returned HTTP ${response.status}.`,
      );
    }

    const buffer = await response.arrayBuffer();
    if (buffer.byteLength !== end - start) {
      throw new Error(
        `Parquet range request returned ${buffer.byteLength} bytes, expected ${end - start}.`,
      );
    }

    this._completedRangeCache.set(cacheKey, buffer);
    return buffer;
  }

  /**
   * Adapt this reader to Hyparquet's slice-based `AsyncBuffer`.
   * Apply the supplied signal to network reads.
   * Return completed cache entries without a new abortable request.
   */
  asAsyncBuffer(signal?: AbortSignal): AsyncBuffer {
    return {
      byteLength: this.byteLength,
      slice: (start, end = this.byteLength) => this.read(start, end, signal),
    };
  }

  /**
   * Clear completed ranges.
   * Do not cancel requests that are in progress.
   */
  clear(): void {
    this._completedRangeCache.clear();
  }
}

/** Reject unsafe integers and invalid half-open ranges before any HTTP request. */
function validateRange(start: number, end: number, byteLength: number): void {
  if (
    !Number.isSafeInteger(start) ||
    !Number.isSafeInteger(end) ||
    start < 0 ||
    end <= start ||
    end > byteLength
  ) {
    throw new Error(`Invalid Parquet byte range [${start}, ${end}).`);
  }
}

import type { AsyncBuffer } from "hyparquet";

export interface ParquetRangeReader {
  readonly byteLength: number;
  read(start: number, end: number, signal?: AbortSignal): Promise<ArrayBuffer>;
  asAsyncBuffer(signal?: AbortSignal): AsyncBuffer;
  clear(): void;
}

/**
 * Provides validated HTTP range reads for a single Parquet file and caches completed requests.
 * It owns transport and `AsyncBuffer` adaptation so Parquet consumers can work with byte ranges instead of HTTP responses.
 */
export class HttpParquetRangeReader implements ParquetRangeReader {
  readonly byteLength: number;

  private readonly _completedRangeCache = new Map<string, ArrayBuffer>();

  constructor(
    private readonly _url: string,
    byteLength: number,
  ) {
    this.byteLength = byteLength;
  }

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

  asAsyncBuffer(signal?: AbortSignal): AsyncBuffer {
    return {
      byteLength: this.byteLength,
      slice: (start, end = this.byteLength) => this.read(start, end, signal),
    };
  }

  clear(): void {
    this._completedRangeCache.clear();
  }
}

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

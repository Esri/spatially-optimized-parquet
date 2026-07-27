import esriConfig from "@arcgis/core/config";
import type { RequestInterceptor, RequestOptions } from "@arcgis/core/request/types";

import { parseRangeHeader } from "./parquetFileLayout";
import type { ParquetDownloadStore } from "./parquetDownloadStore";

export class ParquetRequestInterceptor {
  private readonly requestIds = new WeakMap<RequestOptions, string>();
  private readonly interceptor: RequestInterceptor;

  constructor(
    url: string,
    private readonly store: ParquetDownloadStore,
  ) {
    this.interceptor = {
      urls: url,
      before: ({ requestOptions }) => {
        const range = parseRangeHeader(requestOptions.headers?.range);
        if (!range) {
          return;
        }

        this.requestIds.set(requestOptions, this.store.startRange(range));
      },
      after: ({ requestOptions }) => {
        if (!requestOptions) {
          return;
        }

        const requestId = this.requestIds.get(requestOptions);
        if (requestId) {
          this.store.completeRange(requestId);
        }
      },
      error: (error) => {
        const requestOptions = getRequestOptions(error.details);
        if (!requestOptions) {
          return;
        }

        const requestId = this.requestIds.get(requestOptions);
        if (requestId) {
          this.store.failRange(requestId);
        }
      },
    };
  }

  install(): void {
    esriConfig.request.interceptors.push(this.interceptor);
  }

  dispose(): void {
    const index = esriConfig.request.interceptors.indexOf(this.interceptor);
    if (index !== -1) {
      esriConfig.request.interceptors.splice(index, 1);
    }
  }
}

function getRequestOptions(details: unknown): RequestOptions | null {
  if (
    typeof details === "object" &&
    details !== null &&
    "requestOptions" in details &&
    typeof details.requestOptions === "object" &&
    details.requestOptions !== null
  ) {
    return details.requestOptions as RequestOptions;
  }

  return null;
}

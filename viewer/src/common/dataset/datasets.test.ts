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

import { describe, expect, it } from "vitest";

import {
  createCustomUrlDataset,
  createPortalItemDataset,
  getPortalDomain,
  validateNetworkUrl,
} from "./datasets";

describe("dataset factories", () => {
  it("accepts absolute HTTP and HTTPS URLs without requiring a parquet suffix", () => {
    expect(validateNetworkUrl("https://example.com/data")).toBe(
      "https://example.com/data",
    );
    expect(validateNetworkUrl("http://example.com/file.bin")).toBe(
      "http://example.com/file.bin",
    );
  });

  it("rejects empty, relative, and non-network URLs", () => {
    expect(() => validateNetworkUrl("")).toThrow("URL is required.");
    expect(() => validateNetworkUrl("/data.parquet")).toThrow(
      "URL must be an absolute URL.",
    );
    expect(() => validateNetworkUrl("file:///tmp/data.parquet")).toThrow(
      "URL must use HTTP or HTTPS.",
    );
  });

  it("creates a Custom URL dataset with file-backed map defaults", () => {
    const dataset = createCustomUrlDataset("https://example.com/data");

    expect(dataset.kind).toBe("custom-url");
    expect(dataset.parquet).toEqual({
      type: "url",
      url: "https://example.com/data",
    });
    expect(dataset.source).toBe("--");
  });

  it("creates a Portal Item dataset with portal and item identity", () => {
    const dataset = createPortalItemDataset(
      "example.maps.arcgis.com",
      "item-id",
    );

    expect(dataset.kind).toBe("portal-item");
    expect(dataset.parquet).toEqual({
      type: "portal-item",
      portalUrl: "https://example.maps.arcgis.com",
      itemId: "item-id",
    });
    expect(dataset.source).toBe("ArcGIS Portal item");
    expect(dataset.sourceUrl).toBe(
      "https://example.maps.arcgis.com/home/item.html?id=item-id",
    );
  });

  it("creates the default portal item page source URL", () => {
    const dataset = createPortalItemDataset(
      "jsapi.maps.arcgis.com",
      "5efaf71a6e064e9ea4e67821166c61cd",
    );

    expect(dataset.sourceUrl).toBe(
      "https://jsapi.maps.arcgis.com/home/item.html?id=5efaf71a6e064e9ea4e67821166c61cd",
    );
  });

  it("normalizes portal domains to canonical HTTPS URLs", () => {
    const dataset = createPortalItemDataset("example.com", "item-id");

    expect(dataset.parquet.portalUrl).toBe("https://example.com");
    expect(getPortalDomain(dataset.parquet.portalUrl)).toBe("example.com");
  });

  it("rejects portal paths and credentials", () => {
    expect(() =>
      createPortalItemDataset("example.com/portal", "item-id")
    ).toThrow("only a domain name");
    expect(() =>
      createPortalItemDataset("user@example.com", "item-id")
    ).toThrow("only a domain name");
  });
});

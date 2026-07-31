import { describe, expect, it } from "vitest";

import {
  createCustomUrlDataset,
  createPortalItemDataset,
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
      "https://example.maps.arcgis.com",
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
      "https://jsapi.maps.arcgis.com/",
      "5efaf71a6e064e9ea4e67821166c61cd",
    );

    expect(dataset.sourceUrl).toBe(
      "https://jsapi.maps.arcgis.com/home/item.html?id=5efaf71a6e064e9ea4e67821166c61cd",
    );
  });

  it("preserves hosted portal paths in the item page source URL", () => {
    const dataset = createPortalItemDataset(
      "https://example.com/portal/",
      "item-id",
    );

    expect(dataset.sourceUrl).toBe(
      "https://example.com/portal/home/item.html?id=item-id",
    );
  });
});

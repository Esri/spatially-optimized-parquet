import { describe, expect, it, vi } from "vitest";

vi.mock("@arcgis/core/layers/support/ParquetFilesData", () => ({
  default: class MockParquetFilesData {
    constructor(readonly properties: unknown) {}
  },
}));

vi.mock("@arcgis/core/layers/support/ParquetPortalItemData", () => ({
  default: class MockParquetPortalItemData {
    constructor(readonly properties: unknown) {}
  },
}));

import {
  createCustomUrlDataset,
  createPortalItemDataset,
  datasets,
} from "../common/dataset/datasets";
import { createParquetLayerData } from "./createParquetLayerData";

describe("createParquetLayerData", () => {
  it("creates file data for preset and custom URL datasets", () => {
    const presetData = createParquetLayerData(datasets[0]);
    const customData = createParquetLayerData(
      createCustomUrlDataset("https://example.com/data"),
    );

    expect("properties" in presetData && presetData.properties).toEqual({
      urls: [datasets[0].parquet.url],
    });
    expect("properties" in customData && customData.properties).toEqual({
      urls: ["https://example.com/data"],
    });
  });

  it("sets portal item ID and portal URL on portal data", () => {
    const data = createParquetLayerData(
      createPortalItemDataset(
        "https://example.maps.arcgis.com",
        "item-id",
      ),
    );

    expect("properties" in data && data.properties).toEqual({
      portalItem: {
        id: "item-id",
        portal: { url: "https://example.maps.arcgis.com" },
      },
    });
  });
});

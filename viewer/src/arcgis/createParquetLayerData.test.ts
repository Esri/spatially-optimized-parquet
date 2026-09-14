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

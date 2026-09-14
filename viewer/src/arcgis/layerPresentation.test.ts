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

import type ParquetLayer from "@arcgis/core/layers/ParquetLayer";
import {
  applyLayerPresentation,
  areLayerPresentationsEqual,
  type LayerPresentation,
} from "./layerPresentation";

describe("applyLayerPresentation", () => {
  it("sets every presentation field in one call", () => {
    const set = vi.fn();
    const layer = { set } as unknown as ParquetLayer;
    const presentation = {
      effect: null,
      featureEffect: null,
      popupEnabled: false,
      renderer: undefined,
      visible: true,
    } satisfies LayerPresentation;

    applyLayerPresentation(layer, presentation);

    expect(set).toHaveBeenCalledOnce();
    expect(set).toHaveBeenCalledWith(presentation);
    expect(set.mock.calls[0]?.[0]).toEqual({
      effect: null,
      featureEffect: null,
      popupEnabled: false,
      renderer: undefined,
      visible: true,
    });
  });

  describe("areLayerPresentationsEqual", () => {
    it("compares every atomic presentation field", () => {
      const presentation = {
        effect: null,
        featureEffect: null,
        popupEnabled: true,
        renderer: undefined,
        visible: true,
      } satisfies LayerPresentation;

      expect(areLayerPresentationsEqual(presentation, presentation)).toBe(true);
      expect(
        areLayerPresentationsEqual(presentation, {
          ...presentation,
          popupEnabled: false,
        }),
      ).toBe(false);
    });
  });
});

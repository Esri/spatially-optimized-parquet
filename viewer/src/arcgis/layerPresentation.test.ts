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

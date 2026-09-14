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

import type ParquetLayer from "@arcgis/core/layers/ParquetLayer";
import type { ParquetLayerProperties } from "@arcgis/core/layers/ParquetLayer";

export interface LayerPresentation {
  effect: ParquetLayer["effect"];
  featureEffect: ParquetLayer["featureEffect"];
  popupEnabled: ParquetLayer["popupEnabled"];
  renderer: ParquetLayer["renderer"];
  visible: ParquetLayer["visible"];
}

export type LayerPresentationChange = Partial<LayerPresentation>;

export type LayerPresentationProperties = Pick<
  ParquetLayerProperties,
  "effect" | "featureEffect" | "popupEnabled" | "renderer" | "visible"
>;

interface AtomicLayerPresentationWriter {
  set(properties: LayerPresentation): unknown;
}

export function readLayerPresentation(
  layer: ParquetLayer | null,
): LayerPresentation {
  if (!layer) {
    return {
      effect: null,
      featureEffect: null,
      popupEnabled: true,
      renderer: null,
      visible: true,
    };
  }

  return {
    effect: layer.effect,
    featureEffect: layer.featureEffect,
    popupEnabled: layer.popupEnabled,
    renderer: layer.renderer,
    visible: layer.visible,
  };
}

export function applyLayerPresentation(
  layer: ParquetLayer,
  presentation: LayerPresentation,
): void {
  // Preserve the SDK's atomic Accessor update despite the 5.2 next typings omitting set().
  (layer as ParquetLayer & AtomicLayerPresentationWriter).set(presentation);
}

export function areLayerPresentationsEqual(
  left: LayerPresentation,
  right: LayerPresentation,
): boolean {
  return left.effect === right.effect &&
    left.featureEffect === right.featureEffect &&
    left.popupEnabled === right.popupEnabled &&
    left.renderer === right.renderer &&
    left.visible === right.visible;
}

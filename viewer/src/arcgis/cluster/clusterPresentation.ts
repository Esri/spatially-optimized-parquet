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
import FeatureEffect from "@arcgis/core/layers/support/FeatureEffect";

import type { LayerPresentation } from "../layerPresentation";
import type { ClusterRowGroupSelection } from "./clusterRowGroup";

export interface ActiveClusterRenderer {
  layer: ParquetLayer;
  level: number;
  renderer: NonNullable<ParquetLayer["renderer"]>;
}

export type ClusterRendererState =
  | { type: "inactive" }
  | {
      type: "loading";
      active: ActiveClusterRenderer | null;
      requestedLevel: number;
    }
  | { type: "ready"; active: ActiveClusterRenderer }
  | {
      type: "error";
      active: ActiveClusterRenderer | null;
      error: Error;
      requestedLevel: number | null;
    };

interface ClusterPresentationOptions {
  enabled: boolean;
  layer: ParquetLayer;
  normalPresentation: LayerPresentation;
  rendererState: ClusterRendererState;
  rowGroup: ClusterRowGroupSelection | null;
}

const clusterIncludedEffect = "drop-shadow(3px, 3px, 10px)";
const clusterExcludedEffect = "grayscale(100%) brightness(35%)";

export function resolveClusterPresentation({
  enabled,
  layer,
  normalPresentation,
  rendererState,
  rowGroup,
}: ClusterPresentationOptions): LayerPresentation {
  const activeRenderer = enabled
    ? getActiveClusterRenderer(rendererState, layer)
    : null;
  if (!activeRenderer) {
    return normalPresentation;
  }

  return {
    effect: null,
    featureEffect: createClusterFeatureEffect(layer.objectIdField, rowGroup),
    popupEnabled: false,
    renderer: activeRenderer.renderer,
    visible: true,
  };
}

export function getActiveClusterRenderer(
  state: ClusterRendererState,
  layer: ParquetLayer,
): ActiveClusterRenderer | null {
  const active = state.type === "inactive" ? null : state.active;
  return active?.layer === layer ? active : null;
}

function createClusterFeatureEffect(
  objectIdField: string,
  rowGroup: ClusterRowGroupSelection | null,
): FeatureEffect | null {
  return rowGroup
    ? new FeatureEffect({
        filter: {
          where: [
            `${objectIdField} >= ${rowGroup.objectIdStart}`,
            `${objectIdField} < ${rowGroup.objectIdEnd}`,
          ].join(" AND "),
        },
        includedEffect: clusterIncludedEffect,
        excludedEffect: clusterExcludedEffect,
      })
    : null;
}

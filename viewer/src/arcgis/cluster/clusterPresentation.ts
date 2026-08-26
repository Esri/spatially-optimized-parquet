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

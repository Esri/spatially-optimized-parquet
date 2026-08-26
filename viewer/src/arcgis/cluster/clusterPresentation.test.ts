import { describe, expect, it, vi } from "vitest";

import type ParquetLayer from "@arcgis/core/layers/ParquetLayer";
import {
  applyLayerPresentation,
  type LayerPresentation,
} from "../layerPresentation";
import {
  type ActiveClusterRenderer,
  type ClusterRendererState,
  resolveClusterPresentation,
} from "./clusterPresentation";

const normalRenderer = { id: "normal" } as unknown as NonNullable<
  ParquetLayer["renderer"]
>;
const clusterRenderer = { id: "cluster" } as unknown as NonNullable<
  ParquetLayer["renderer"]
>;
const normalPresentation = {
  effect: "bloom(0.15)",
  featureEffect: null,
  popupEnabled: true,
  renderer: normalRenderer,
  visible: true,
} satisfies LayerPresentation;

describe("resolveClusterPresentation", () => {
  it("keeps the normal presentation during the first renderer load", () => {
    const layer = createLayer();

    expect(
      resolveClusterPresentation({
        enabled: true,
        layer,
        normalPresentation,
        rendererState: {
          type: "loading",
          active: null,
          requestedLevel: 10,
        },
        rowGroup: null,
      }),
    ).toBe(normalPresentation);
  });

  it("activates every Cluster presentation field together", () => {
    const layer = createLayer();

    expect(
      resolveClusterPresentation({
        enabled: true,
        layer,
        normalPresentation,
        rendererState: createReadyState(layer),
        rowGroup: null,
      }),
    ).toEqual({
      effect: null,
      featureEffect: null,
      popupEnabled: false,
      renderer: clusterRenderer,
      visible: true,
    });
  });

  it("keeps the active Cluster renderer during a later level load", () => {
    const layer = createLayer();
    const active = createActiveRenderer(layer);

    expect(
      resolveClusterPresentation({
        enabled: true,
        layer,
        normalPresentation,
        rendererState: {
          type: "loading",
          active,
          requestedLevel: 11,
        },
        rowGroup: null,
      }).renderer,
    ).toBe(clusterRenderer);
  });

  it("adds row-group emphasis to the complete Cluster presentation", () => {
    const layer = createLayer();
    const presentation = resolveClusterPresentation({
      enabled: true,
      layer,
      normalPresentation,
      rendererState: createReadyState(layer),
      rowGroup: { objectIdStart: 100, objectIdEnd: 200 },
    });

    expect(presentation.featureEffect?.filter?.where).toBe(
      "OBJECTID >= 100 AND OBJECTID < 200",
    );
    expect(presentation.renderer).toBe(clusterRenderer);
    expect(presentation.effect).toBeNull();
  });

  it("keeps normal state after a first-load failure", () => {
    const layer = createLayer();

    expect(
      resolveClusterPresentation({
        enabled: true,
        layer,
        normalPresentation,
        rendererState: {
          type: "error",
          active: null,
          error: new Error("failed"),
          requestedLevel: 10,
        },
        rowGroup: null,
      }),
    ).toBe(normalPresentation);
  });

  it("keeps the last Cluster renderer after a replacement failure", () => {
    const layer = createLayer();

    expect(
      resolveClusterPresentation({
        enabled: true,
        layer,
        normalPresentation,
        rendererState: {
          type: "error",
          active: createActiveRenderer(layer),
          error: new Error("failed"),
          requestedLevel: 11,
        },
        rowGroup: null,
      }).renderer,
    ).toBe(clusterRenderer);
  });

  it("restores the normal presentation when Cluster is disabled", () => {
    const layer = createLayer();

    expect(
      resolveClusterPresentation({
        enabled: false,
        layer,
        normalPresentation,
        rendererState: createReadyState(layer),
        rowGroup: null,
      }),
    ).toBe(normalPresentation);
  });

  it("commits the Census-style Cluster transition in one layer update", () => {
    const set = vi.fn();
    const layer = {
      objectIdField: "OBJECTID",
      set,
    } as unknown as ParquetLayer;
    const presentation = resolveClusterPresentation({
      enabled: true,
      layer,
      normalPresentation,
      rendererState: createReadyState(layer),
      rowGroup: null,
    });

    applyLayerPresentation(layer, presentation);

    expect(set).toHaveBeenCalledOnce();
    expect(set).toHaveBeenCalledWith({
      effect: null,
      featureEffect: null,
      popupEnabled: false,
      renderer: clusterRenderer,
      visible: true,
    });
  });
});

function createLayer(): ParquetLayer {
  return { objectIdField: "OBJECTID" } as ParquetLayer;
}

function createReadyState(layer: ParquetLayer): ClusterRendererState {
  return { type: "ready", active: createActiveRenderer(layer) };
}

function createActiveRenderer(layer: ParquetLayer): ActiveClusterRenderer {
  return { layer, level: 10, renderer: clusterRenderer };
}

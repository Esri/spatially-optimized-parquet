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
import {
  type RefObject,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import type { ParquetFileDiagnostics } from "../../parquet/fileLayout";
import {
  applyLayerPresentation,
  areLayerPresentationsEqual,
  type LayerPresentation,
  readLayerPresentation,
} from "../layerPresentation";
import type { ClusterLevel } from "./clusterLevelCatalog";
import { ClusterPageRendererStore } from "./clusterPageRenderer";
import {
  type ActiveClusterRenderer,
  getActiveClusterRenderer,
  resolveClusterPresentation,
  type ClusterRendererState,
} from "./clusterPresentation";
import {
  resolveClusterRowGroup,
  type ClusterRowGroupSelection,
} from "./clusterRowGroup";

export type ClusterModeStatus =
  | { type: "idle" }
  | { type: "loading" }
  | { type: "ready" }
  | { type: "error"; error: Error };

interface ClusterModeOptions {
  enabled: boolean;
  files: readonly ParquetFileDiagnostics[];
  layer: ParquetLayer | null;
  level: ClusterLevel | null;
  mapElementRef: RefObject<HTMLArcgisMapElement | null>;
  normalPresentation: LayerPresentation;
  parquetSource: unknown | null;
  preparedRenderer: ActiveClusterRenderer | null;
}

interface AppliedPresentation {
  layer: ParquetLayer;
  presentation: LayerPresentation;
}

export function useClusterMode({
  enabled,
  files,
  layer,
  level,
  mapElementRef,
  normalPresentation,
  parquetSource,
  preparedRenderer,
}: ClusterModeOptions): ClusterModeStatus {
  const [rendererState, setRendererState] = useState<ClusterRendererState>({
    type: "inactive",
  });
  const [rowGroup, setRowGroup] =
    useState<ClusterRowGroupSelection | null>(null);
  const appliedPresentationRef = useRef<AppliedPresentation | null>(null);
  const rendererStore = useMemo(
    () =>
      layer && parquetSource
        ? new ClusterPageRendererStore(
            parquetSource,
            files,
            layer.objectIdField,
          )
        : null,
    [files, layer, parquetSource],
  );
  const resolvedRendererState = useMemo<ClusterRendererState>(() => {
    const preparedRendererForLayer =
      preparedRenderer?.layer === layer ? preparedRenderer : null;
    return layer &&
        !getActiveClusterRenderer(rendererState, layer) &&
        preparedRendererForLayer
      ? { type: "ready", active: preparedRendererForLayer }
      : rendererState;
  }, [layer, preparedRenderer, rendererState]);
  const resolvedPresentation = useMemo(
    () =>
      layer
        ? resolveClusterPresentation({
            enabled,
            layer,
            normalPresentation,
            rendererState: resolvedRendererState,
            rowGroup,
          })
        : null,
    [
      enabled,
      layer,
      normalPresentation,
      resolvedRendererState,
      rowGroup,
    ],
  );
  const activeRenderer = layer
    ? getActiveClusterRenderer(resolvedRendererState, layer)
    : null;

  useLayoutEffect(() => {
    if (!layer || !resolvedPresentation) {
      appliedPresentationRef.current = null;
      return;
    }

    const applied = appliedPresentationRef.current;
    const currentPresentation = applied?.layer === layer
      ? applied.presentation
      : readLayerPresentation(layer);
    if (areLayerPresentationsEqual(currentPresentation, resolvedPresentation)) {
      appliedPresentationRef.current = {
        layer,
        presentation: resolvedPresentation,
      };
      return;
    }

    applyLayerPresentation(layer, resolvedPresentation);
    appliedPresentationRef.current = {
      layer,
      presentation: resolvedPresentation,
    };
  }, [layer, resolvedPresentation]);

  useEffect(() => {
    setRowGroup(null);
    if (!enabled) {
      setRendererState({ type: "inactive" });
    }
  }, [enabled, layer]);

  useEffect(() => {
    const view = mapElementRef.current?.view;
    if (!enabled || !layer || !view || !activeRenderer) {
      return;
    }

    let requestVersion = 0;
    const clickHandle = view.on("click", async (event) => {
      const currentRequest = ++requestVersion;
      const hitTest = await view.hitTest(event, { include: layer });
      if (currentRequest !== requestVersion) {
        return;
      }

      const hit = hitTest.results.find(
        (result) => result.type === "graphic" && result.layer === layer,
      );
      const objectId = hit?.type === "graphic"
        ? hit.graphic.attributes?.[layer.objectIdField]
        : null;
      setRowGroup(
        typeof objectId === "number"
          ? resolveClusterRowGroup(files, objectId)
          : null,
      );
    });

    return () => {
      requestVersion += 1;
      clickHandle.remove();
    };
  }, [activeRenderer, enabled, files, layer, mapElementRef]);

  useEffect(() => {
    if (!enabled || !layer) {
      return;
    }
    if (!level || !rendererStore) {
      if (files.length === 0) {
        return;
      }
      const error = new Error(
        level
          ? "The Parquet diagnostics source is unavailable."
          : "The dataset has no multiscale level shared by every file.",
      );
      setRendererState((current) => ({
        type: "error",
        active: getActiveClusterRenderer(current, layer),
        error,
        requestedLevel: level?.level ?? null,
      }));
      return;
    }
    if (activeRenderer?.level === level.level) {
      setRendererState({ type: "ready", active: activeRenderer });
      return;
    }

    let cancelled = false;
    setRendererState((current) => ({
      type: "loading",
      active: getActiveClusterRenderer(current, layer),
      requestedLevel: level.level,
    }));
    void rendererStore.load(level).then((renderer) => {
      if (cancelled) {
        return;
      }
      setRendererState({
        type: "ready",
        active: { layer, level: level.level, renderer },
      });
    }).catch((error: unknown) => {
      if (cancelled) {
        return;
      }
      setRendererState((current) => ({
        type: "error",
        active: getActiveClusterRenderer(current, layer),
        error: error instanceof Error
          ? error
          : new Error("Failed to load the Cluster renderer."),
        requestedLevel: level.level,
      }));
    });

    return () => {
      cancelled = true;
    };
  }, [
    activeRenderer,
    enabled,
    files.length,
    layer,
    level,
    rendererStore,
  ]);

  if (!enabled) {
    return { type: "idle" };
  }
  if (rendererState.type === "error") {
    return { type: "error", error: rendererState.error };
  }
  if (rendererState.type === "ready") {
    return { type: "ready" };
  }
  return { type: "loading" };
}

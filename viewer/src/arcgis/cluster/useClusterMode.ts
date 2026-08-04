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
import type { DatasetLayerPresentation } from "../profiles/profiles";
import type { ClusterLevel } from "./clusterLevelCatalog";
import { ClusterPageRendererStore } from "./clusterPageRenderer";
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
  normalPresentation: DatasetLayerPresentation;
  parquetSource: unknown | null;
}

const clusterIncludedEffect = "drop-shadow(3px, 3px, 10px)";
const clusterExcludedEffect = "grayscale(100%) brightness(35%)";

export function useClusterMode({
  enabled,
  files,
  layer,
  level,
  mapElementRef,
  normalPresentation,
  parquetSource,
}: ClusterModeOptions): ClusterModeStatus {
  const [status, setStatus] = useState<ClusterModeStatus>({ type: "idle" });
  const [rowGroup, setRowGroup] =
    useState<ClusterRowGroupSelection | null>(null);
  const activeRendererLayerRef = useRef<ParquetLayer | null>(null);
  const normalPresentationRef = useRef(normalPresentation);
  normalPresentationRef.current = normalPresentation;
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

  useLayoutEffect(() => {
    if (!layer || !enabled) {
      return;
    }

    const popupEnabled = layer.popupEnabled;
    const visible = layer.visible;
    layer.popupEnabled = false;
    layer.featureEffect = null;
    if (activeRendererLayerRef.current !== layer) {
      layer.visible = false;
    }

    return () => {
      layer.popupEnabled = popupEnabled;
      layer.visible = visible;
      if (activeRendererLayerRef.current === layer) {
        activeRendererLayerRef.current = null;
      }
    };
  }, [enabled, layer]);

  useLayoutEffect(() => {
    if (!layer || enabled) {
      return;
    }
    layer.renderer = normalPresentation.renderer;
    layer.featureEffect = normalPresentation.featureEffect;
  }, [enabled, layer, normalPresentation]);

  useLayoutEffect(() => {
    if (!layer) {
      return;
    }
    layer.featureEffect = enabled && rowGroup
      ? {
          filter: {
            where: [
              `${layer.objectIdField} >= ${rowGroup.objectIdStart}`,
              `${layer.objectIdField} < ${rowGroup.objectIdEnd}`,
            ].join(" AND "),
          },
          includedEffect: clusterIncludedEffect,
          excludedEffect: clusterExcludedEffect,
        }
      : enabled
        ? null
        : normalPresentation.featureEffect;
  }, [enabled, layer, normalPresentation.featureEffect, rowGroup]);

  useEffect(() => {
    setRowGroup(null);
    if (!enabled) {
      setStatus({ type: "idle" });
    }
  }, [enabled, layer]);

  useEffect(() => {
    const view = mapElementRef.current?.view;
    if (!enabled || !layer || !view) {
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
  }, [enabled, files, layer, mapElementRef]);

  useEffect(() => {
    if (!enabled || !layer) {
      return;
    }
    if (!level || !rendererStore) {
      if (files.length === 0) {
        return;
      }
      layer.renderer = normalPresentationRef.current.renderer;
      layer.visible = true;
      setStatus({
        type: "error",
        error: new Error(
          level
            ? "The Parquet diagnostics source is unavailable."
            : "The dataset has no multiscale level shared by every file.",
        ),
      });
      return;
    }

    let cancelled = false;
    setStatus({ type: "loading" });
    void rendererStore.load(level).then((renderer) => {
      if (cancelled) {
        return;
      }
      layer.renderer = renderer;
      layer.visible = true;
      activeRendererLayerRef.current = layer;
      setStatus({ type: "ready" });
    }).catch((error: unknown) => {
      if (cancelled) {
        return;
      }
      layer.renderer = normalPresentationRef.current.renderer;
      layer.visible = true;
      setStatus({
        type: "error",
        error: error instanceof Error
          ? error
          : new Error("Failed to load the Cluster renderer."),
      });
    });

    return () => {
      cancelled = true;
    };
  }, [
    enabled,
    files.length,
    layer,
    level,
    rendererStore,
  ]);

  return status;
}

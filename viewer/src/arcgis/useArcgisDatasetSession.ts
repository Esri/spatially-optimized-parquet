import ParquetLayer from "@arcgis/core/layers/ParquetLayer";
import ParquetFilesData from "@arcgis/core/layers/support/ParquetFilesData";
import * as reactiveUtils from "@arcgis/core/core/reactiveUtils";
import { useEffect, useRef, useState, type RefObject } from "react";

import type { Dataset } from "../common/dataset/datasets";
import type {
  DatasetEffectLayer,
  DatasetMapProfile,
} from "./profiles/profiles";
import {
  parseParquetDiagnosticsSnapshot,
  parseParquetRangeReadEvent,
  resolveParquetDiagnosticsSource,
  type ArcgisEventHandle,
} from "./diagnostics";
import { ParquetDownloadSession } from "./file-explorer/download/ParquetDownloadSession";
import type { RowGroupBounds } from "../parquet/rowGroupBounds";

export interface ArcgisDatasetSessionResult {
  readonly download: ParquetDownloadSession;
  readonly featureCount: number | null;
  readonly layer: ParquetLayer | null;
  readonly parquetSource: unknown | null;
  readonly rowGroupBounds: readonly RowGroupBounds[] | null;
}

export interface ArcgisDatasetSessionOptions {
  readonly dataset: Dataset;
  readonly mapElementRef: RefObject<HTMLArcgisMapElement | null>;
  readonly mapReady: boolean;
  readonly profile: DatasetMapProfile;
}

const defaultCenter: [number, number] = [-98, 39];
const defaultScale = 25_000_000;

export function useArcgisDatasetSession({
  dataset,
  mapElementRef,
  mapReady,
  profile,
}: ArcgisDatasetSessionOptions): ArcgisDatasetSessionResult {
  const downloadRef = useRef<ParquetDownloadSession | null>(null);
  const layerRef = useRef<ParquetLayer | null>(null);
  const [featureCount, setFeatureCount] = useState<number | null>(null);
  const [layer, setLayer] = useState<ParquetLayer | null>(null);
  const [parquetSource, setParquetSource] = useState<unknown | null>(null);
  const [rowGroupBounds, setRowGroupBounds] =
    useState<readonly RowGroupBounds[] | null>(null);

  if (!downloadRef.current) {
    downloadRef.current = new ParquetDownloadSession();
  }

  const download = downloadRef.current;

  useEffect(() => {
    const mapElement = mapElementRef.current;
    const map = mapElement?.map;
    if (!mapReady || !mapElement || !map || !dataset.url) {
      return;
    }

    setFeatureCount(null);
    setRowGroupBounds(null);
    setParquetSource(null);
    download.reset();
    mapElement.center = dataset.center ?? defaultCenter;
    mapElement.scale = dataset.scale ?? defaultScale;
    map.layers.removeAll();
    const parquetLayer = new ParquetLayer({
      title: dataset.name,
      copyright: dataset.source,
      data: new ParquetFilesData({ urls: [dataset.url] }),
      maxScale: dataset.maxScale,
      ...profile.layerProperties,
    });
    layerRef.current = parquetLayer;
    setLayer(parquetLayer);
    map.add(parquetLayer);

    let disposed = false;
    let layerViewWatcher: { remove(): void } | undefined;
    let rangeReadHandle: ArcgisEventHandle | undefined;
    const profileLayerCleanup = hasDatasetEffectLayer(parquetLayer)
      ? profile.configureLayer?.(parquetLayer)
      : undefined;
    const reportDiagnosticsError = (message: string, error: unknown) => {
      if (disposed) {
        return;
      }
      download.reportError(error instanceof Error ? error : new Error(message));
      console.error(message, error);
    };

    const initializeLayer = async () => {
      try {
        await parquetLayer.when();
        if (disposed) {
          return;
        }

        try {
          const diagnosticsSource = resolveParquetDiagnosticsSource(parquetLayer);
          setParquetSource(diagnosticsSource);
          rangeReadHandle = diagnosticsSource.on("range-read", (event) => {
            try {
              download.recordRangeRead(parseParquetRangeReadEvent(event));
            } catch (error) {
              reportDiagnosticsError("Failed to parse a Parquet range event.", error);
            }
          });
          void diagnosticsSource.getDiagnosticsSnapshot().then((snapshotValue) => {
            if (disposed) {
              return;
            }
            const snapshot = parseParquetDiagnosticsSnapshot(snapshotValue);
            download.loadDiagnostics(snapshot);
            setRowGroupBounds(download.topology.getSnapshot().rowGroupBounds);
          }).catch((error: unknown) => {
            reportDiagnosticsError(
              "Failed to load the Parquet diagnostics snapshot.",
              error,
            );
          });
        } catch (error) {
          reportDiagnosticsError(
            "Parquet layer source diagnostics are unavailable.",
            error,
          );
        }

        const layerView = await mapElement.whenLayerView(parquetLayer);
        if (disposed) {
          return;
        }
        let layerViewUpdateVersion = 0;
        const refreshLayerViewCount = async () => {
          const updateVersion = layerViewUpdateVersion;
          try {
            const count = await layerView.queryFeatureCount();
            if (!disposed && !layerView.updating && updateVersion === layerViewUpdateVersion) {
              setFeatureCount(count);
            }
          } catch (error) {
            if (!disposed) {
              console.error("Failed to query LayerView feature count.", error);
            }
          }
        };
        layerViewWatcher = reactiveUtils.watch(
          () => layerView.updating,
          (updating) => {
            layerViewUpdateVersion += 1;
            if (!updating) {
              void refreshLayerViewCount();
            }
          },
        );
        if (!layerView.updating) {
          void refreshLayerViewCount();
        }
      } catch (error) {
        if (!disposed) {
          console.error("Failed to query Parquet feature counts.", error);
        }
      }
    };
    void initializeLayer();

    return () => {
      disposed = true;
      rangeReadHandle?.remove();
      layerViewWatcher?.remove();
      profileLayerCleanup?.();
      if (layerRef.current === parquetLayer) {
        layerRef.current = null;
      }
      setParquetSource(null);
      setLayer((current) => current === parquetLayer ? null : current);
      map.layers.removeAll();
    };
  }, [dataset, download, mapElementRef, mapReady, profile]);

  useEffect(() => {
    const mapElement = mapElementRef.current;
    if (!mapElement || !layer || !profile.mountMapComponents) {
      return;
    }
    return profile.mountMapComponents({ mapElement, layer });
  }, [layer, mapElementRef, profile]);

  return {
    download,
    featureCount,
    layer,
    parquetSource,
    rowGroupBounds,
  };
}

function hasDatasetEffectLayer(layer: object): layer is DatasetEffectLayer {
  return "effect" in layer;
}

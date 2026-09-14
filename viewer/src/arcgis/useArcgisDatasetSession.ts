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

import * as reactiveUtils from "@arcgis/core/core/reactiveUtils";
import ParquetLayer from "@arcgis/core/layers/ParquetLayer";
import {
  useEffect,
  useReducer,
  useRef,
  type RefObject,
} from "react";

import type { Dataset } from "../common/dataset/datasets";
import type { DatasetRouteViewpoint } from "../common/dataset/datasetRoute";
import { deriveClusterLevels } from "./cluster/clusterLevelCatalog";
import { ClusterPageRendererStore } from "./cluster/clusterPageRenderer";
import {
  type ActiveClusterRenderer,
  resolveClusterPresentation,
} from "./cluster/clusterPresentation";
import { createParquetLayerData } from "./createParquetLayerData";
import {
  reduceDatasetSessionState,
  type DatasetSessionState,
} from "./datasetSessionState";
import {
  parseParquetDiagnosticsSnapshot,
  parseParquetRangeReadEvent,
  resolveParquetDiagnosticsSource,
  type ArcgisEventHandle,
  type ParquetDiagnosticsSnapshot,
} from "./diagnostics";
import { ParquetDatasetDownloadSession } from "./file-explorer/download/ParquetDatasetDownloadSession";
import { inferCustomExtent } from "./inferCustomExtent";
import {
  applyLayerPresentation,
  type LayerPresentation,
  readLayerPresentation,
} from "./layerPresentation";
import type {
  DatasetMapProfile,
} from "./profiles/profiles";

export interface ArcgisDatasetSessionResult {
  readonly dataset: Dataset;
  readonly download: ParquetDatasetDownloadSession;
  readonly featureCount: number | null;
  readonly layer: ParquetLayer | null;
  readonly loadError: Error | null;
  readonly loading: boolean;
  readonly normalPresentation: LayerPresentation;
  readonly parquetSource: unknown | null;
  readonly preparedClusterRenderer: ActiveClusterRenderer | null;
}

export interface ArcgisDatasetSessionOptions {
  readonly clusterEnabled: boolean;
  readonly dataset: Dataset;
  readonly mapElementRef: RefObject<HTMLArcgisMapElement | null>;
  readonly mapReady: boolean;
  readonly profile: DatasetMapProfile;
  readonly viewpoint: DatasetRouteViewpoint | null;
}

interface LoadedDatasetSession {
  dataset: Dataset;
  disposed: boolean;
  download: ParquetDatasetDownloadSession;
  featureCount: number | null;
  layer: ParquetLayer | null;
  layerViewWatcher?: { remove(): void };
  normalPresentation: LayerPresentation;
  parquetSource: unknown | null;
  preparedClusterRenderer: ActiveClusterRenderer | null;
  rangeReadHandle?: ArcgisEventHandle;
}

const defaultCenter: [number, number] = [-98, 39];
const defaultScale = 25_000_000;

export function useArcgisDatasetSession({
  clusterEnabled,
  dataset,
  mapElementRef,
  mapReady,
  profile,
  viewpoint,
}: ArcgisDatasetSessionOptions): ArcgisDatasetSessionResult {
  const clusterEnabledRef = useRef(clusterEnabled);
  clusterEnabledRef.current = clusterEnabled;
  const viewpointRef = useRef(viewpoint);
  viewpointRef.current = viewpoint;
  const initialSessionRef = useRef<LoadedDatasetSession | null>(null);
  if (!initialSessionRef.current) {
    initialSessionRef.current = createEmptyDatasetSession(dataset);
  }

  const [state, dispatch] = useReducer(
    reduceDatasetSessionState<LoadedDatasetSession>,
    {
      committed: initialSessionRef.current,
      loadError: null,
      loading: false,
      requestVersion: 0,
    } satisfies DatasetSessionState<LoadedDatasetSession>,
  );
  const committedSessionRef = useRef(state.committed);
  const requestVersionRef = useRef(0);

  useEffect(() => {
    const mapElement = mapElementRef.current;
    const map = mapElement?.map;
    if (!mapReady || !mapElement || !map) {
      return;
    }

    const requestVersion = ++requestVersionRef.current;
    const candidate = createDatasetCandidate(dataset, profile);
    const prepareCluster = clusterEnabledRef.current;
    const requestedViewpoint = viewpointRef.current;
    let cancelled = false;
    dispatch({ type: "request-started", requestVersion });

    const publishCandidate = () => {
      if (committedSessionRef.current === candidate) {
        dispatch({ type: "session-refreshed", session: candidate });
      }
    };

    const loadCandidate = async () => {
      try {
        const parquetLayer = candidate.layer;
        if (!parquetLayer) {
          throw new Error("Parquet layer candidate is unavailable.");
        }

        await parquetLayer.load();
        if (cancelled || requestVersion !== requestVersionRef.current) {
          disposeDatasetSession(candidate);
          return;
        }

        candidate.normalPresentation = readLayerPresentation(parquetLayer);
        const diagnosticsReady = attachDatasetDiagnostics(
          candidate,
          publishCandidate,
        );
        const diagnostics = candidate.dataset.kind === "preset" &&
            !prepareCluster
          ? null
          : await diagnosticsReady;
        if (prepareCluster) {
          await prepareCandidateCluster(candidate, diagnostics);
        }
        if (cancelled || requestVersion !== requestVersionRef.current) {
          disposeDatasetSession(candidate);
          return;
        }

        const previousSession = committedSessionRef.current;
        map.layers.removeAll();
        disposeDatasetSession(previousSession);
        await navigateToDataset(
          candidate,
          mapElement,
          diagnostics,
          requestedViewpoint,
        );
        if (cancelled || requestVersion !== requestVersionRef.current) {
          disposeDatasetSession(candidate);
          return;
        }

        map.add(parquetLayer);
        committedSessionRef.current = candidate;
        dispatch({
          type: "request-succeeded",
          requestVersion,
          session: candidate,
        });

        void initializeLayerView(candidate, mapElement, publishCandidate);
      } catch (error) {
        disposeDatasetSession(candidate);
        if (cancelled || requestVersion !== requestVersionRef.current) {
          return;
        }
        dispatch({
          type: "request-failed",
          requestVersion,
          error: toError(error, "Failed to load the Parquet dataset."),
        });
      }
    };
    void loadCandidate();

    return () => {
      cancelled = true;
      if (committedSessionRef.current !== candidate) {
        disposeDatasetSession(candidate);
      }
    };
  }, [dataset, mapElementRef, mapReady, profile]);

  useEffect(() => {
    const mapElement = mapElementRef.current;
    return () => {
      const session = committedSessionRef.current;
      if (session.layer) {
        mapElement?.map?.layers.remove(session.layer);
      }
      disposeDatasetSession(session);
    };
  }, [mapElementRef]);

  return {
    dataset: state.committed.dataset,
    download: state.committed.download,
    featureCount: state.committed.featureCount,
    layer: state.committed.layer,
    loadError: state.loadError,
    loading: state.loading,
    normalPresentation: state.committed.normalPresentation,
    parquetSource: state.committed.parquetSource,
    preparedClusterRenderer: state.committed.preparedClusterRenderer,
  };
}

function createEmptyDatasetSession(
  dataset: Dataset,
): LoadedDatasetSession {
  return {
    dataset,
    disposed: false,
    download: new ParquetDatasetDownloadSession(),
    featureCount: null,
    layer: null,
    normalPresentation: readLayerPresentation(null),
    parquetSource: null,
    preparedClusterRenderer: null,
  };
}

function createDatasetCandidate(
  dataset: Dataset,
  profile: DatasetMapProfile,
): LoadedDatasetSession {
  const layer = new ParquetLayer({
    title: dataset.name,
    copyright: dataset.source,
    data: createParquetLayerData(dataset),
    maxScale: dataset.maxScale,
    ...profile.layerProperties,
    ...profile.initialPresentation,
  });
  const session = createEmptyDatasetSession(dataset);
  session.layer = layer;
  return session;
}

async function prepareCandidateCluster(
  session: LoadedDatasetSession,
  diagnostics: ParquetDiagnosticsSnapshot | null,
): Promise<void> {
  const layer = session.layer;
  if (!layer || !session.parquetSource || !diagnostics) {
    throw new Error("The Parquet diagnostics source is unavailable.");
  }

  const files = session.download.files.map(({ diagnostics: file }) => file);
  const level = deriveClusterLevels(files).at(-1);
  if (!level) {
    throw new Error(
      "The dataset has no multiscale level shared by every file.",
    );
  }

  const rendererStore = new ClusterPageRendererStore(
    session.parquetSource,
    files,
    layer.objectIdField,
  );
  const renderer = await rendererStore.load(level);
  const active = { layer, level: level.level, renderer };
  session.preparedClusterRenderer = active;
  applyLayerPresentation(
    layer,
    resolveClusterPresentation({
      enabled: true,
      layer,
      normalPresentation: session.normalPresentation,
      rendererState: { type: "ready", active },
      rowGroup: null,
    }),
  );
}

function attachDatasetDiagnostics(
  session: LoadedDatasetSession,
  publish: () => void,
): Promise<ParquetDiagnosticsSnapshot | null> {
  const layer = session.layer;
  if (!layer) {
    return Promise.resolve(null);
  }

  const reportDiagnosticsError = (message: string, error: unknown) => {
    if (session.disposed) {
      return;
    }
    session.download.reportError(toError(error, message));
    console.error(message, error);
  };

  try {
    const diagnosticsSource = resolveParquetDiagnosticsSource(layer);
    session.parquetSource = diagnosticsSource;
    session.rangeReadHandle = diagnosticsSource.on("range-read", (event) => {
      try {
        session.download.recordRangeRead(parseParquetRangeReadEvent(event));
      } catch (error) {
        reportDiagnosticsError("Failed to parse a Parquet range event.", error);
      }
    });
    return diagnosticsSource.getDiagnosticsSnapshot().then(async (snapshotValue) => {
      if (session.disposed) {
        return null;
      }
      const snapshot = parseParquetDiagnosticsSnapshot(snapshotValue);
      await session.download.loadDiagnostics(snapshot);
      if (session.disposed) {
        return null;
      }
      publish();
      return snapshot;
    }).catch((error: unknown) => {
      reportDiagnosticsError(
        "Failed to load the Parquet diagnostics snapshot.",
        error,
      );
      return null;
    });
  } catch (error) {
    reportDiagnosticsError(
      "Parquet layer source diagnostics are unavailable.",
      error,
    );
    return Promise.resolve(null);
  }
}

async function initializeLayerView(
  session: LoadedDatasetSession,
  mapElement: HTMLArcgisMapElement,
  publish: () => void,
): Promise<void> {
  const layer = session.layer;
  if (!layer) {
    return;
  }

  try {
    const layerView = await mapElement.whenLayerView(layer);
    if (session.disposed) {
      return;
    }

    let layerViewUpdateVersion = 0;
    const refreshLayerViewCount = async () => {
      const updateVersion = layerViewUpdateVersion;
      try {
        const count = await layerView.queryFeatureCount();
        if (
          !session.disposed &&
          !layerView.updating &&
          updateVersion === layerViewUpdateVersion
        ) {
          session.featureCount = count;
          publish();
        }
      } catch (error) {
        if (!session.disposed) {
          console.error("Failed to query LayerView feature count.", error);
        }
      }
    };

    session.layerViewWatcher = reactiveUtils.watch(
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
    if (!session.disposed) {
      console.error("Failed to initialize the Parquet LayerView.", error);
    }
  }
}

async function navigateToDataset(
  session: LoadedDatasetSession,
  mapElement: HTMLArcgisMapElement,
  diagnostics: ParquetDiagnosticsSnapshot | null = null,
  viewpoint: DatasetRouteViewpoint | null = null,
): Promise<void> {
  if (session.disposed) {
    return;
  }

  try {
    if (viewpoint) {
      await mapElement.view.goTo(viewpoint, { animate: false });
      return;
    }

    if (session.dataset.kind === "preset") {
      await mapElement.view.goTo(
        {
          center: session.dataset.center ?? defaultCenter,
          scale: session.dataset.scale ?? defaultScale,
        },
        { animate: false },
      );
      return;
    }

    const inferredExtent = session.layer && diagnostics
      ? await inferCustomExtent(session.layer, diagnostics)
      : null;
    if (session.disposed) {
      return;
    }
    const navigationExtent = inferredExtent ?? session.layer?.fullExtent;
    if (!navigationExtent) {
      return;
    }
    await mapElement.view.goTo(navigationExtent, { animate: false });
  } catch (error) {
    if (!session.disposed) {
      console.error("Failed to navigate to the Parquet full extent.", error);
    }
  }
}

function disposeDatasetSession(session: LoadedDatasetSession): void {
  if (session.disposed) {
    return;
  }

  session.disposed = true;
  session.rangeReadHandle?.remove();
  session.layerViewWatcher?.remove();
  session.download.dispose();
  session.layer?.destroy();
}

function toError(error: unknown, fallbackMessage: string): Error {
  return error instanceof Error ? error : new Error(fallbackMessage);
}

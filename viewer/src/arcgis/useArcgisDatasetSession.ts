import * as reactiveUtils from "@arcgis/core/core/reactiveUtils";
import ParquetLayer from "@arcgis/core/layers/ParquetLayer";
import {
  useEffect,
  useReducer,
  useRef,
  type RefObject,
} from "react";

import type { Dataset } from "../common/dataset/datasets";
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
import type {
  DatasetEffectLayer,
  DatasetMapProfile,
  DatasetProfileCleanup,
} from "./profiles/profiles";

export interface ArcgisDatasetSessionResult {
  readonly dataset: Dataset;
  readonly download: ParquetDatasetDownloadSession;
  readonly featureCount: number | null;
  readonly layer: ParquetLayer | null;
  readonly loadError: Error | null;
  readonly loading: boolean;
  readonly parquetSource: unknown | null;
}

export interface ArcgisDatasetSessionOptions {
  readonly dataset: Dataset;
  readonly mapElementRef: RefObject<HTMLArcgisMapElement | null>;
  readonly mapReady: boolean;
  readonly profile: DatasetMapProfile;
}

interface LoadedDatasetSession {
  dataset: Dataset;
  disposed: boolean;
  download: ParquetDatasetDownloadSession;
  featureCount: number | null;
  layer: ParquetLayer | null;
  layerViewWatcher?: { remove(): void };
  parquetSource: unknown | null;
  profile: DatasetMapProfile;
  profileComponentCleanup?: DatasetProfileCleanup;
  profileLayerCleanup?: DatasetProfileCleanup;
  rangeReadHandle?: ArcgisEventHandle;
}

const defaultCenter: [number, number] = [-98, 39];
const defaultScale = 25_000_000;

export function useArcgisDatasetSession({
  dataset,
  mapElementRef,
  mapReady,
  profile,
}: ArcgisDatasetSessionOptions): ArcgisDatasetSessionResult {
  const initialSessionRef = useRef<LoadedDatasetSession | null>(null);
  if (!initialSessionRef.current) {
    initialSessionRef.current = createEmptyDatasetSession(dataset, profile);
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

        const diagnosticsReady = attachDatasetDiagnostics(
          candidate,
          publishCandidate,
        );
        const previousSession = committedSessionRef.current;
        map.layers.removeAll();
        disposeDatasetSession(previousSession);
        map.add(parquetLayer);
        committedSessionRef.current = candidate;
        candidate.profileComponentCleanup = profile.mountMapComponents?.({
          mapElement,
          layer: parquetLayer,
        });
        dispatch({
          type: "request-succeeded",
          requestVersion,
          session: candidate,
        });

        void initializeLayerView(candidate, mapElement, publishCandidate);
        if (candidate.dataset.kind === "preset") {
          void navigateToDataset(candidate, mapElement);
        } else {
          void diagnosticsReady.then((snapshot) => {
            void navigateToDataset(candidate, mapElement, snapshot);
          });
        }
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
    parquetSource: state.committed.parquetSource,
  };
}

function createEmptyDatasetSession(
  dataset: Dataset,
  profile: DatasetMapProfile,
): LoadedDatasetSession {
  return {
    dataset,
    disposed: false,
    download: new ParquetDatasetDownloadSession(),
    featureCount: null,
    layer: null,
    parquetSource: null,
    profile,
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
  });
  const session = createEmptyDatasetSession(dataset, profile);
  session.layer = layer;
  session.profileLayerCleanup = hasDatasetEffectLayer(layer)
    ? profile.configureLayer?.(layer)
    : undefined;
  return session;
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
): Promise<void> {
  if (session.disposed) {
    return;
  }

  if (session.dataset.kind === "preset") {
    mapElement.center = session.dataset.center ?? defaultCenter;
    mapElement.scale = session.dataset.scale ?? defaultScale;
    return;
  }

  try {
    const inferredExtent = session.layer && diagnostics
      ? await inferCustomExtent(session.layer, diagnostics)
      : null;
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
  session.profileComponentCleanup?.();
  session.profileLayerCleanup?.();
  session.download.dispose();
  session.layer?.destroy();
}

function hasDatasetEffectLayer(layer: object): layer is DatasetEffectLayer {
  return "effect" in layer;
}

function toError(error: unknown, fallbackMessage: string): Error {
  return error instanceof Error ? error : new Error(fallbackMessage);
}

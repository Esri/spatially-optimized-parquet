import { useCallback, useEffect, useState } from "react";

import { type Dataset, datasets } from "./datasets";
import {
  createDatasetRouteUrl,
  createViewpointRouteUrl,
  type DatasetRouteViewpoint,
  resolveDatasetRoute,
} from "./datasetRoute";

interface DatasetRouteState {
  dataset: Dataset;
  selectDataset(dataset: Dataset): void;
  setViewpoint(viewpoint: DatasetRouteViewpoint): void;
  viewpoint: DatasetRouteViewpoint | null;
}

export function useDatasetRoute(): DatasetRouteState {
  const [route, setRoute] = useState(readCurrentDatasetRoute);

  useEffect(() => {
    const restoreDatasetRoute = () => {
      setRoute(readCurrentDatasetRoute());
    };
    window.addEventListener("popstate", restoreDatasetRoute);
    return () => window.removeEventListener("popstate", restoreDatasetRoute);
  }, []);

  const selectDataset = useCallback((nextDataset: Dataset) => {
    const nextUrl = createDatasetRouteUrl(
      new URL(window.location.href),
      nextDataset,
    );
    if (nextUrl.href !== window.location.href) {
      window.history.pushState(window.history.state, "", nextUrl);
    }
    setRoute({ dataset: nextDataset, viewpoint: null });
  }, []);

  const setViewpoint = useCallback((viewpoint: DatasetRouteViewpoint) => {
    const nextUrl = createViewpointRouteUrl(
      new URL(window.location.href),
      viewpoint,
    );
    window.history.replaceState(window.history.state, "", nextUrl);
  }, []);

  return {
    dataset: route.dataset,
    selectDataset,
    setViewpoint,
    viewpoint: route.viewpoint,
  };
}

function readCurrentDatasetRoute() {
  try {
    return resolveDatasetRoute(new URL(window.location.href));
  } catch (error) {
    console.error("Failed to resolve the dataset URL.", error);
    return { dataset: datasets[0], viewpoint: null };
  }
}

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

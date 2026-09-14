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

import {
  createViewerRouteUrl,
  resolveViewerRoute,
  type ViewerType,
} from "./viewerRoute";

interface ViewerRouteState {
  selectViewer(viewer: ViewerType): void;
  viewer: ViewerType;
}

export function useViewerRoute(): ViewerRouteState {
  const [viewer, setViewer] = useState(readCurrentViewerRoute);

  useEffect(() => {
    const restoreViewerRoute = () => {
      setViewer(readCurrentViewerRoute());
    };
    window.addEventListener("popstate", restoreViewerRoute);
    return () => window.removeEventListener("popstate", restoreViewerRoute);
  }, []);

  const selectViewer = useCallback((nextViewer: ViewerType) => {
    const nextUrl = createViewerRouteUrl(
      new URL(window.location.href),
      nextViewer,
    );
    if (nextUrl.href !== window.location.href) {
      window.history.pushState(window.history.state, "", nextUrl);
    }
    setViewer(nextViewer);
  }, []);

  return { selectViewer, viewer };
}

function readCurrentViewerRoute(): ViewerType {
  return resolveViewerRoute(new URL(window.location.href));
}

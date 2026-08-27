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

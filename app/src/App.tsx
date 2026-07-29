import { lazy, Suspense, useState } from "react";

import { ApplicationHeader } from "./ApplicationHeader";
import { ArcgisViewer } from "./ArcgisViewer";
import type { ViewerType } from "./viewerType";

const MaplibreViewer = lazy(() => import("./maplibre/MaplibreViewer"));

export function App() {
  const [viewer, setViewer] = useState<ViewerType>("arcgis");

  return (
    <calcite-shell className="calcite-mode-dark">
      <ApplicationHeader viewer={viewer} onViewerChange={setViewer} />
      <Suspense fallback={null}>
        {viewer === "arcgis" ? <ArcgisViewer /> : <MaplibreViewer />}
      </Suspense>
    </calcite-shell>
  );
}

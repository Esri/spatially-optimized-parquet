import { lazy, Suspense } from "react";

import { ArcgisViewer } from "./arcgis/ArcgisViewer";
import type { ViewerType } from "./viewerRoute";

const MaplibreViewer = lazy(
  () => import("./maplibre/MapLibreViewer"),
);


export function Viewer({ viewer }: { viewer: ViewerType }) {
  return (
    <Suspense fallback={null}>
      {viewer === "arcgis" ? <ArcgisViewer /> : <MaplibreViewer />}
    </Suspense>
  );
}
import { lazy, Suspense } from "react";

import { ArcgisViewer } from "./arcgis/ArcgisViewer";

const MaplibreViewer = lazy(
  () => import("./maplibre/MapLibreViewer"),
);

export type ViewerType = "arcgis" | "maplibre";

export function Viewer({ viewer }: { viewer: ViewerType }) {
  return (
    <Suspense fallback={null}>
      {viewer === "arcgis" ? <ArcgisViewer /> : <MaplibreViewer />}
    </Suspense>
  );
}

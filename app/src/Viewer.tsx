import { lazy, Suspense } from "react";

import { ArcgisViewer } from "./arcgis/ArcgisViewer";

const MaplibreViewer = lazy(
  () => import("./maplibre/MaplibreViewer"),
);

export type ViewerType = "arcgis" | "maplibre";

export function Viewer({ viewer }: { viewer: ViewerType }) {
  return (
    <Suspense fallback={null}>
      {viewer === "arcgis" ? <ArcgisViewer /> : <MaplibreViewer />}
    </Suspense>
  );
}

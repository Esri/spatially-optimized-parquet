import { lazy, Suspense } from "react";

import { ArcgisViewer } from "../features/map-workspace/arcgis/ArcgisViewer";
import type { ViewerType } from "./viewerType";

const MaplibreViewer = lazy(
  () => import("../features/map-workspace/maplibre/MaplibreViewer"),
);

export function Viewer({ viewer }: { viewer: ViewerType }) {
  return (
    <Suspense fallback={null}>
      {viewer === "arcgis" ? <ArcgisViewer /> : <MaplibreViewer />}
    </Suspense>
  );
}

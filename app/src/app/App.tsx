import { useState } from "react";

import { ApplicationHeader } from "./ApplicationHeader";
import { Viewer } from "./Viewer";
import type { ViewerType } from "./viewerType";

export function App() {
  const [viewer, setViewer] = useState<ViewerType>("arcgis");

  return (
    <calcite-shell className="calcite-mode-dark">
      <ApplicationHeader viewer={viewer} onViewerChange={setViewer} />
      <Viewer viewer={viewer} />
    </calcite-shell>
  );
}

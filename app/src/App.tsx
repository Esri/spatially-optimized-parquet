import { useState } from "react";

import { ApplicationHeader } from "./ApplicationHeader";
import { Viewer, type ViewerType } from "./Viewer";
import styles from "./App.module.css";

export function App() {
  const [viewer, setViewer] = useState<ViewerType>("arcgis");

  return (
    <calcite-shell className={`${styles.shell} calcite-mode-dark`}>
      <ApplicationHeader viewer={viewer} onViewerChange={setViewer} />
      <Viewer viewer={viewer} />
    </calcite-shell>
  );
}

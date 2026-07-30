import { useState } from "react";

import { AppHeader } from "./AppHeader";
import { Viewer, type ViewerType } from "./Viewer";
import styles from "./App.module.css";

export function App() {
  const [viewer, setViewer] = useState<ViewerType>("arcgis");

  return (
    <calcite-shell className={`${styles.shell} calcite-mode-dark`}>
      <AppHeader viewer={viewer} onViewerChange={setViewer} />
      <Viewer viewer={viewer} />
    </calcite-shell>
  );
}

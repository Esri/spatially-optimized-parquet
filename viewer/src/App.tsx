import { AppHeader } from "./AppHeader";
import { useViewerRoute } from "./useViewerRoute";
import { Viewer } from "./Viewer";
import styles from "./App.module.css";

export function App() {
  const { selectViewer, viewer } = useViewerRoute();

  return (
    <calcite-shell className={`${styles.shell} calcite-mode-dark`}>
      <AppHeader viewer={viewer} onViewerChange={selectViewer} />
      <Viewer viewer={viewer} />
    </calcite-shell>
  );
}

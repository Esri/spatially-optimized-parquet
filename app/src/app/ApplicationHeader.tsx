import type { ViewerType } from "./viewerType";

interface ApplicationHeaderProps {
  viewer: ViewerType;
  onViewerChange(viewer: ViewerType): void;
}

function viewerToggleLabel(viewer: ViewerType): string {
  return viewer === "arcgis" ? "MapLibre Starter Code" : "ArcGIS Maps SDK";
}

export function ApplicationHeader({
  viewer,
  onViewerChange,
}: ApplicationHeaderProps) {
  const nextViewer = viewer === "arcgis" ? "maplibre" : "arcgis";
  const toggleLabel = viewerToggleLabel(viewer);

  return (
    <calcite-navigation slot="header">
      <calcite-navigation-logo
        className="application-logo"
        heading="Spatially Optimized Parquet"
        slot="logo"
      />
      <div className="application-sdk-label" slot="content-start">
        <span>{viewer === "arcgis" ? "ArcGIS Maps SDK" : "MapLibre GL JS"}</span>
      </div>
      <calcite-menu slot="content-end" label="Application links">
        <calcite-menu-item
          className="viewer-switch-menu-item"
          text={toggleLabel}
          label={toggleLabel}
          iconStart="code"
          onClick={() => onViewerChange(nextViewer)}
        />
        <calcite-menu-item
          text="GitHub"
          label="Open GitHub"
          iconStart="launch"
          href="https://github.com"
          target="_blank"
          rel="noopener noreferrer"
        />
      </calcite-menu>
    </calcite-navigation>
  );
}

import type { ViewerType } from "./viewerType";

interface ApplicationHeaderProps {
  viewer: ViewerType;
  onViewerChange(viewer: ViewerType): void;
}

function viewerDescription(viewer: ViewerType): string {
  return viewer === "arcgis"
    ? "ArcGIS Maps SDK for JavaScript"
    : "MapLibre GL JS";
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
        description={viewerDescription(viewer)}
        heading="Spatially Optimized Parquet"
        slot="logo"
      />
      <calcite-menu slot="content-end" label="Application links">
        <calcite-menu-item
          text="Documentation"
          label="Documentation"
          iconStart="book"
        />
        <calcite-menu-item
          text="GitHub"
          label="Open GitHub"
          iconStart="launch"
          href="https://github.com"
          target="_blank"
          rel="noopener noreferrer"
        />
        <calcite-menu-item
          text={toggleLabel}
          label={toggleLabel}
          iconStart="rotate"
          onClick={() => onViewerChange(nextViewer)}
        />
      </calcite-menu>
    </calcite-navigation>
  );
}

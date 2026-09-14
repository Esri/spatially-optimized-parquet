// Copyright 2026 Esri
//
// Licensed under the Apache License Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import type { ViewerType } from "./viewerRoute";
import styles from "./AppHeader.module.css";

interface AppHeaderProps {
  viewer: ViewerType;
  onViewerChange(viewer: ViewerType): void;
}

export function AppHeader({
  viewer,
  onViewerChange,
}: AppHeaderProps) {
  const nextViewer = viewer === "arcgis" ? "maplibre" : "arcgis";
  const toggleLabel = viewerToggleLabel(viewer);

  return (
    <calcite-navigation slot="header">
      <calcite-navigation-logo
        className={styles.applicationLogo}
        heading="Spatially Optimized Parquet"
        slot="logo"
      />
      <div className={styles.applicationSdkLabel} slot="content-start">
        <span>{viewer === "arcgis" ? "ArcGIS Maps SDK" : "MapLibre GL JS"}</span>
      </div>
      <calcite-menu slot="content-end" label="Application links">
        <calcite-menu-item
          className={styles.viewerSwitchMenuItem}
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

function viewerToggleLabel(viewer: ViewerType): string {
  return viewer === "arcgis" ? "MapLibre Example" : "ArcGIS Maps SDK";
}

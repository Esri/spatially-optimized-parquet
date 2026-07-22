import { useState } from "react";

type Dataset = "usa" | "canada";

const datasetView = {
  usa: {
    center: [-98, 39],
    label: "United States",
    zoom: 4,
  },
  canada: {
    center: [-106, 57],
    label: "Canada",
    zoom: 3,
  },
} as const;

const stubChunkState = Array.from({ length: 60 }, (_, index) => {
  if (index < 15) {
    return "cached";
  }

  if (index < 18) {
    return "active";
  }

  if (index < 21) {
    return "loading";
  }

  return "empty";
});

export function App() {
  const [dataset, setDataset] = useState<Dataset>("usa");
  const activeView = datasetView[dataset];

  return (
    <calcite-shell className="calcite-mode-dark">
      <calcite-navigation slot="header">
        <calcite-navigation-logo
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
            text="MapLibre Starter Code"
            label="MapLibre Starter Code"
            iconStart="rotate"
          />
        </calcite-menu>
      </calcite-navigation>

      <main className="grid-container">
        <calcite-panel className="grid-map">
          <calcite-menu
            className="dataset-menu"
            slot="header-actions-start"
            label="Dataset selection"
          >
            <calcite-menu-item
              text={`Dataset: ${activeView.label}`}
              label={`Selected dataset: ${activeView.label}`}
              iconStart="layers"
            >
              <calcite-menu-item
                slot="submenu-item"
                text="USA"
                label="Select USA dataset"
                active={dataset === "usa"}
                onClick={() => setDataset("usa")}
              />
              <calcite-menu-item
                slot="submenu-item"
                text="Canada"
                label="Select Canada dataset"
                active={dataset === "canada"}
                onClick={() => setDataset("canada")}
              />
            </calcite-menu-item>
          </calcite-menu>
          <span className="metric-heading" slot="header-actions-end">
            Feature Count
          </span>
          <calcite-label
            className="panel-metric"
            slot="header-actions-end"
            layout="inline"
          >
            Layer
            <strong>2.5M</strong>
          </calcite-label>
          <calcite-label
            className="panel-metric"
            slot="header-actions-end"
            layout="inline"
          >
            LayerView
            <strong>2M</strong>
          </calcite-label>
          <span
            className="header-divider"
            slot="header-actions-end"
            aria-hidden="true"
          />
          <calcite-switch
            className="debug-switch"
            slot="header-actions-end"
            label="Debug"
            labelTextEnd="Debug"
          />
          <arcgis-map
            basemap="dark-gray-vector"
            center={[...activeView.center]}
            zoom={activeView.zoom}
            aria-label={`Dark gray basemap of ${activeView.label}`}
          >
            <arcgis-zoom slot="top-left" />
          </arcgis-map>
        </calcite-panel>

        <calcite-panel className="grid-panel-desktop" heading="Details">
          <calcite-chip
            className="details-count-chip"
            slot="header-actions-end"
            scale="s"
            label="Details count: 54"
          >
            54
          </calcite-chip>
          <calcite-block
            heading="About"
            iconStart="map"
            expanded
            collapsible
          >
            <calcite-label layout="inline-space-between">
              Center
              <strong>{activeView.label}</strong>
            </calcite-label>
            <calcite-label layout="inline-space-between">
              Initial zoom
              <strong>{activeView.zoom}</strong>
            </calcite-label>
          </calcite-block>

          <calcite-block
            heading="File download"
            iconStart="grid"
            expanded
            collapsible
          >
            <div className="occupancy-grid-frame">
              <div
                className="occupancy-grid"
                role="img"
                aria-label="Stub grid showing Parquet chunk occupancy"
              >
                {stubChunkState.map((state, index) => (
                  <span
                    className={`chunk ${state}`}
                    title={`Chunk ${index + 1}: ${state}`}
                    key={index}
                  />
                ))}
              </div>
            </div>

            <div className="occupancy-legend" aria-label="Occupancy legend">
              <span className="legend-item">
                <span className="swatch" />
                Empty
              </span>
              <span className="legend-item">
                <span className="swatch loading" />
                Loading
              </span>
              <span className="legend-item">
                <span className="swatch cached" />
                Cached
              </span>
              <span className="legend-item">
                <span className="swatch active" />
                Current
              </span>
            </div>
          </calcite-block>

          <calcite-block
            heading="File statistics"
            iconStart="graph-bar"
            expanded
            collapsible
          >
            <div className="file-stats-grid">
              <div className="file-stat">
                <div className="file-stat-label">Bytes downloaded</div>
                <div className="file-stat-value">
                  12.1<span className="file-stat-unit">MB</span>
                </div>
              </div>
              <div className="file-stat">
                <div className="file-stat-label">Total file size</div>
                <div className="file-stat-value">
                  40.3<span className="file-stat-unit">MB</span>
                </div>
              </div>
              <div className="file-stat">
                <div className="file-stat-label">File downloaded</div>
                <div className="file-stat-value">
                  30<span className="file-stat-unit">%</span>
                </div>
              </div>
              <div className="file-stat">
                <div className="file-stat-label">Chunks cached</div>
                <div className="file-stat-value">
                  18<span className="file-stat-unit">/ 60</span>
                </div>
              </div>
            </div>

            <calcite-progress
              className="download-progress"
              type="determinate"
              value={30}
              text="30% of file downloaded"
              label="Total file download progress"
            />
          </calcite-block>
        </calcite-panel>
      </main>
    </calcite-shell>
  );
}

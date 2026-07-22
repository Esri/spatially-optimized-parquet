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

const stubRowGroup = [
  {
    label: "RG0",
    column: [
      { label: "C0", range: ["cached", "cached", "active"] },
      { label: "C1", range: ["cached", "loading"] },
      { label: "C2", range: ["cached", "cached", "cached", "empty"] },
      { label: "C3", range: ["empty", "empty"] },
    ],
  },
  {
    label: "RG1",
    column: [
      { label: "C0", range: ["active", "active", "cached"] },
      { label: "C1", range: ["loading", "empty", "empty"] },
      { label: "C2", range: ["cached", "cached"] },
      { label: "C3", range: ["empty", "empty", "empty", "empty"] },
    ],
  },
  {
    label: "RG2",
    column: [
      { label: "C0", range: ["cached", "cached"] },
      { label: "C1", range: ["empty", "empty", "empty"] },
      { label: "C2", range: ["empty", "empty"] },
      { label: "C3", range: ["empty", "empty", "empty"] },
    ],
  },
] as const;

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
            heading="File download"
            iconStart="grid"
            expanded
            collapsible
          >
            <div className="occupancy-grid-frame">
              <div
                className="occupancy-flow"
                role="img"
                aria-label="Stub map of Parquet row groups, columns, and downloaded byte ranges"
              >
                {stubRowGroup.map((rowGroup) => (
                  <section className="row-group" key={rowGroup.label}>
                    <strong className="row-group-label">{rowGroup.label}</strong>
                    <div className="row-group-flow">
                      {rowGroup.column.map((column) => (
                        <span className="column-range" key={column.label}>
                          <span className="column-label">{column.label}</span>
                          {column.range.map((state, index) => (
                            <span
                              className={`chunk ${state}`}
                              title={`${rowGroup.label} ${column.label} range ${index + 1}: ${state}`}
                              key={index}
                            />
                          ))}
                        </span>
                      ))}
                    </div>
                  </section>
                ))}
                <section className="footer-range">
                  <strong className="column-label">FT</strong>
                  <span
                    className="chunk cached"
                    title="Parquet footer byte range: cached"
                  />
                </section>
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

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import "@arcgis/map-components/components/arcgis-map";
import "@arcgis/map-components/components/arcgis-zoom";
import "@esri/calcite-components/components/calcite-navigation";
import "@esri/calcite-components/components/calcite-navigation-logo";
import "@esri/calcite-components/components/calcite-block";
import "@esri/calcite-components/components/calcite-chip";
import "@esri/calcite-components/components/calcite-label";
import "@esri/calcite-components/components/calcite-menu";
import "@esri/calcite-components/components/calcite-menu-item";
import "@esri/calcite-components/components/calcite-panel";
import "@esri/calcite-components/components/calcite-progress";
import "@esri/calcite-components/components/calcite-shell";
import "@esri/calcite-components/components/calcite-switch";
import "@arcgis/map-components/main.css";
import "@esri/calcite-components/main.css";

import { App } from "./App";
import "./styles/app.css";

const rootElement = document.getElementById("root");

if (!rootElement) {
  throw new Error("Missing root element.");
}

createRoot(rootElement).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

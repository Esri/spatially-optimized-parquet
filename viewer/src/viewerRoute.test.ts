import { describe, expect, it } from "vitest";

import {
  createViewerRouteUrl,
  resolveViewerRoute,
} from "./viewerRoute";

describe("viewer routes", () => {
  it("resolves MapLibre from a deployment-base route", () => {
    expect(
      resolveViewerRoute(new URL("https://example.com/base/maplibre/")),
    ).toBe("maplibre");
    expect(
      resolveViewerRoute(new URL("https://example.com/base/maplibre")),
    ).toBe("maplibre");
  });

  it("resolves the deployment base as the ArcGIS viewer", () => {
    expect(
      resolveViewerRoute(new URL("https://example.com/base/")),
    ).toBe("arcgis");
  });

  it("creates a MapLibre route while preserving URL state", () => {
    const url = createViewerRouteUrl(
      new URL("https://example.com/base/?dataset=alaska#map"),
      "maplibre",
    );

    expect(url.pathname).toBe("/base/maplibre/");
    expect(url.search).toBe("?dataset=alaska");
    expect(url.hash).toBe("#map");
  });

  it("returns from MapLibre to the deployment base", () => {
    const url = createViewerRouteUrl(
      new URL("https://example.com/base/maplibre/?dataset=alaska"),
      "arcgis",
    );

    expect(url.pathname).toBe("/base/");
    expect(url.search).toBe("?dataset=alaska");
  });
});

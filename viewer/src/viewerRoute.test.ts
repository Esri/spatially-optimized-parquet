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

import { describe, expect, it } from "vitest";

import {
  createCustomUrlDataset,
  createPortalItemDataset,
  datasets,
} from "./datasets";
import {
  createDatasetRouteUrl,
  createViewpointRouteUrl,
  resolveDatasetRoute,
} from "./datasetRoute";

describe("dataset routes", () => {
  it("resolves preset dataset IDs", () => {
    const { dataset } = resolveDatasetRoute(
      new URL("https://example.com/app?dataset=country-borders"),
    );

    expect(dataset).toBe(
      datasets.find(({ id }) => id === "country-borders"),
    );
  });

  it("resolves custom URL datasets", () => {
    const { dataset } = resolveDatasetRoute(
      new URL(
        "https://example.com/app?dataset=custom&url=https%3A%2F%2Fdata.example.com%2Ffile.parquet%3Fx%3D1",
      ),
    );

    expect(dataset.kind).toBe("custom-url");
    expect(dataset.parquet).toEqual({
      type: "url",
      url: "https://data.example.com/file.parquet?x=1",
    });
  });

  it("resolves portal item datasets", () => {
    const { dataset } = resolveDatasetRoute(
      new URL(
        "https://example.com/app?dataset=portal&portal=example.maps.arcgis.com&id=item-42",
      ),
    );

    expect(dataset.kind).toBe("portal-item");
    expect(dataset.parquet).toEqual({
      type: "portal-item",
      portalUrl: "https://example.maps.arcgis.com",
      itemId: "item-42",
    });
  });

  it("serializes presets while preserving unrelated URL state", () => {
    const url = createDatasetRouteUrl(
      new URL(
        "https://example.com/app?theme=dark&dataset=custom&url=https%3A%2F%2Fold.example.com#map",
      ),
      datasets[1],
    );

    expect(url.pathname).toBe("/app");
    expect(url.hash).toBe("#map");
    expect(url.searchParams.get("theme")).toBe("dark");
    expect(url.searchParams.get("dataset")).toBe(datasets[1].id);
    expect(url.searchParams.has("url")).toBe(false);
    expect(url.searchParams.has("portal")).toBe(false);
    expect(url.searchParams.has("id")).toBe(false);
  });

  it("serializes custom and portal routes with encoded source values", () => {
    const customUrl = createDatasetRouteUrl(
      new URL("https://example.com/app"),
      createCustomUrlDataset("https://data.example.com/file.parquet?x=1&y=2"),
    );
    const portalUrl = createDatasetRouteUrl(
      customUrl,
      createPortalItemDataset(
        "example.com",
        "item id",
      ),
    );

    expect(customUrl.searchParams.get("dataset")).toBe("custom");
    expect(customUrl.searchParams.get("url")).toBe(
      "https://data.example.com/file.parquet?x=1&y=2",
    );
    expect(portalUrl.searchParams.get("dataset")).toBe("portal");
    expect(portalUrl.searchParams.get("portal")).toBe(
      "example.com",
    );
    expect(portalUrl.searchParams.get("id")).toBe("item id");
    expect(portalUrl.searchParams.has("url")).toBe(false);
  });

  it("rejects unknown and incomplete routes", () => {
    expect(() =>
      resolveDatasetRoute(
        new URL("https://example.com/app?dataset=missing"),
      )
    ).toThrow("Unknown dataset route");
    expect(() =>
      resolveDatasetRoute(
        new URL("https://example.com/app?dataset=custom"),
      )
    ).toThrow("Custom dataset URL is required");
    expect(() =>
      resolveDatasetRoute(
        new URL(
          "https://example.com/app?dataset=portal&portal=https%3A%2F%2Fexample.com",
        ),
      )
    ).toThrow("Portal item ID is required");
  });

  it("resolves and serializes map viewpoint state", () => {
    const url = createViewpointRouteUrl(
      new URL("https://example.com/app?dataset=country-borders"),
      { center: [-74.00612345, 40.71281234], scale: 144448.125 },
    );

    expect(url.searchParams.get("center")).toBe("-74.006123,40.712812");
    expect(url.searchParams.get("scale")).toBe("144448.13");
    expect(resolveDatasetRoute(url).viewpoint).toEqual({
      center: [-74.006123, 40.712812],
      scale: 144448.13,
    });
  });

  it("clears stale viewpoint state when selecting a dataset", () => {
    const url = createDatasetRouteUrl(
      new URL(
        "https://example.com/app?dataset=country-borders&center=1%2C2&scale=500",
      ),
      datasets[0],
    );

    expect(url.searchParams.has("center")).toBe(false);
    expect(url.searchParams.has("scale")).toBe(false);
  });

  it("rejects incomplete and invalid viewpoint state", () => {
    expect(() =>
      resolveDatasetRoute(
        new URL(
          "https://example.com/app?dataset=country-borders&center=1%2C2",
        ),
      )
    ).toThrow("center and scale must be provided together");
    expect(() =>
      resolveDatasetRoute(
        new URL(
          "https://example.com/app?dataset=country-borders&center=200%2C2&scale=500",
        ),
      )
    ).toThrow("valid longitude and latitude");
  });
});

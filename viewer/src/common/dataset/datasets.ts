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


export type DatasetId =
  | "census-blocks"
  | "building-footprints-japan"
  | "national-building-database-france"
  | "alaska-3d-hydrography"
  | "unified-schools"
  | "country-borders";

export type DatasetKind = "preset" | "custom-url" | "portal-item";

export type ParquetDatasetSource =
  | {
      type: "url";
      url: string;
    }
  | {
      type: "portal-item";
      portalUrl: string;
      itemId: string;
    };

interface DatasetBase {
  id: string;
  kind: DatasetKind;
  parquet: ParquetDatasetSource;
  name: string;
  source: string;
  sourceUrl?: string;
  outSpatialReference?: 3857;
  bookmarks?: Bookmark[];
  center: [number, number];
  scale: number;
  maxScale?: number;
  basemap?: string;
  spatialReference?: number;
}

export interface PresetDataset extends DatasetBase {
  id: DatasetId;
  kind: "preset";
  parquet: {
    type: "url";
    url: string;
  };
  count: number;
  byteSize: number;
}

export interface CustomUrlDataset extends DatasetBase {
  kind: "custom-url";
  parquet: {
    type: "url";
    url: string;
  };
}

export interface PortalItemDataset extends DatasetBase {
  kind: "portal-item";
  parquet: {
    type: "portal-item";
    portalUrl: string;
    itemId: string;
  };
}

export type Dataset =
  | PresetDataset
  | CustomUrlDataset
  | PortalItemDataset;

export interface Bookmark {
  name: string;
  center: [number, number];
  scale: number;
}

const customDatasetCenter: [number, number] = [-98, 39];
const customDatasetScale = 25_000_000;
export const defaultCustomDatasetUrl =
  "https://stgeadlsv278968f2d.blob.core.windows.net/parquet/sop/0.1/us_schools.parquet";
export const defaultPortalUrl = "jsapi.maps.arcgis.com";
export const defaultPortalItemId = "5efaf71a6e064e9ea4e67821166c61cd";

export const datasets: PresetDataset[] = [
  {
    id: "census-blocks",
    kind: "preset",
    name: "Census Blocks, Demographics",
    source: "United States Census",
    sourceUrl: "https://data.census.gov/",
    count: 11_155_486,
    parquet: {
      type: "url",
      url: "https://fd-stgeadlsv278968f2d-g4hrhshhhuaxgqc7.a02.azurefd.net/sop/0.1/census_blocks.parquet",
    },
    center: [-74.006, 40.68],
    scale: 577_791 / 2,
    byteSize: 12_975_526_811,
    bookmarks: [
      {
        name: "New York City, NY",
        center: [-74.006, 40.7128], 
        scale: 144_448,
      },
      {
        name: "Washington, D.C.",
        center: [-77.03637, 38.89511],
        scale: 144_448,
      },
      {
        name: "Dallas, TX",
        center: [-96.797, 32.7767],
        scale: 144_448,
      },
      {
        name: "Los Angeles, CA",
        center: [-118.2437, 34.0522],
        scale: 144_448,
      }
    ]
  },
  {
    id: "building-footprints-japan",
    kind: "preset",
    name: "Building Footprints, Japan",
    source: "OpenStreetMap contributors, Overture Maps Foundation",
    sourceUrl: "https://overturemaps.org/",
    count: 53_742_978,
    parquet: {
      type: "url",
      url: "https://fd-stgeadlsv278968f2d-g4hrhshhhuaxgqc7.a02.azurefd.net/sop/0.1/japan_buildings.parquet",
    },
    bookmarks: [],
    center: [139.783372, 35.675360],
    scale: 144_448 / 4,
    // maxScale: 144_448 / 2, 
    byteSize: 6_177_950_661,
  },

  {
    id: "national-building-database-france",
    kind: "preset",
    name: "National Building Database, France",
    source: "Centre Scientifique et Technique du Bâtiment",
    sourceUrl: "https://www.data.gouv.fr/datasets/base-de-donnees-nationale-des-batiments",
    count: 32_220_045,
    parquet: {
      type: "url",
      url: "https://fd-stgeadlsv278968f2d-g4hrhshhhuaxgqc7.a02.azurefd.net/sop/0.1/france_buildings.parquet",
    },
    center: [2.3522, 48.8566],
    scale: 144_448 / 2,
    byteSize: 8_034_324_439,
  },

  {
    id: "alaska-3d-hydrography",
    kind: "preset",
    name: "Alaska Hydrography",
    source: "U.S. Geological Survey",
    sourceUrl: "https://www.sciencebase.gov/catalog/item/69743a65d4be0260181a121b" ,
    count: 3_278_591,
    parquet: {
      type: "url",
      url: "https://fd-stgeadlsv278968f2d-g4hrhshhhuaxgqc7.a02.azurefd.net/sop/0.1/hydro_alaska.parquet",
    },
    center: [-149.9003, 61.2181],
    scale: 144_448 * 16,
    basemap: "6178c4387d07481f87539022ab641aeb",
    spatialReference: 4326,
    byteSize: 1_967_053_038,
  },
  {
    id: "country-borders",
    kind: "preset",
    name: "Country Borders, Overture",
    source: "OpenStreetMap contributors, Overture Maps Foundation",
    sourceUrl: "https://overturemaps.org/",
    count: 219,
    parquet: {
      type: "url",
      url: "https://stgeadlsv278968f2d.blob.core.windows.net/parquet/sop/0.1/country_borders.parquet",
    },
    bookmarks: [],
    center: [0, 0],
    scale: 144_448 * 512,
    byteSize: 7.91e+7,
  },
  {
    id: "unified-schools",
    kind: "preset",
    name: "Unified School Districts",
    source: "United States Census",
    sourceUrl: "https://www.census.gov/geographies/mapping-files/time-series/geo/tiger-geopackage-file.html",
    count: 10867,
    parquet: {
      type: "url",
      url: "https://stgeadlsv278968f2d.blob.core.windows.net/parquet/sop/0.1/us_schools.parquet",
    },
    bookmarks: [],
    center: [-77.03, 38.895],
    scale: 144_448 * 64,
    byteSize: 310481871,
  },
]

export function createCustomUrlDataset(url: string): CustomUrlDataset {
  const validatedUrl = validateNetworkUrl(url, "Parquet URL");
  return {
    id: `custom-url:${validatedUrl}`,
    kind: "custom-url",
    parquet: { type: "url", url: validatedUrl },
    name: "Custom URL",
    source: "--",
    center: customDatasetCenter,
    scale: customDatasetScale,
  };
}

export function createPortalItemDataset(
  portalDomain: string,
  itemId: string,
): PortalItemDataset {
  const validatedPortalUrl = createPortalUrl(portalDomain);
  const validatedItemId = itemId.trim();
  if (!validatedItemId) {
    throw new Error("Portal item ID is required.");
  }

  const itemPageUrl = new URL(validatedPortalUrl);
  itemPageUrl.pathname = `${itemPageUrl.pathname.replace(/\/?$/, "/")}home/item.html`;
  itemPageUrl.search = "";
  itemPageUrl.hash = "";
  itemPageUrl.searchParams.set("id", validatedItemId);

  return {
    id: `portal-item:${validatedPortalUrl}:${validatedItemId}`,
    kind: "portal-item",
    parquet: {
      type: "portal-item",
      portalUrl: validatedPortalUrl,
      itemId: validatedItemId,
    },
    name: "Portal Item",
    source: "ArcGIS Portal item",
    sourceUrl: itemPageUrl.href,
    center: customDatasetCenter,
    scale: customDatasetScale,
  };
}

export function getPortalDomain(portalUrl: string): string {
  return new URL(portalUrl).host;
}

export function validateNetworkUrl(value: string, label = "URL"): string {
  const trimmedValue = value.trim();
  if (!trimmedValue) {
    throw new Error(`${label} is required.`);
  }

  let url: URL;
  try {
    url = new URL(trimmedValue);
  } catch {
    throw new Error(`${label} must be an absolute URL.`);
  }

  if (url.protocol !== "http:" && url.protocol !== "https:") {
    throw new Error(`${label} must use HTTP or HTTPS.`);
  }

  return trimmedValue;
}

function createPortalUrl(value: string): string {
  const trimmedValue = value.trim();
  if (!trimmedValue) {
    throw new Error("Portal domain is required.");
  }

  let url: URL;
  try {
    url = new URL(
      trimmedValue.includes("://") ? trimmedValue : `https://${trimmedValue}`,
    );
  } catch {
    throw new Error("Portal domain must be a valid domain.");
  }

  if (
    (url.protocol !== "http:" && url.protocol !== "https:") ||
    !url.hostname ||
    url.username ||
    url.password ||
    (url.pathname !== "/" && url.pathname !== "") ||
    url.search ||
    url.hash
  ) {
    throw new Error("Portal domain must contain only a domain name.");
  }

  url.protocol = "https:";
  return url.origin;
}

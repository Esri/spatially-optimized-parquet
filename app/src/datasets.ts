
export type DatasetId =
  | "census-blocks"
  | "building-footprints-japan"
  | "national-building-database-france"
  | "alaska-3d-hydrography"
  | "country-borders";

export interface Dataset {
  id: DatasetId;
  url: string;
  name: string;
  count: number;
  source: string;
  sourceUrl?: string;
  outSpatialReference?: 3857;
  bookmarks?: Bookmark[];
  center: [number, number];
  scale: number;
  maxScale?: number;
  basemap?: string;
  spatialReference?: number;
  byteSize: number;
}

export interface Bookmark {
  name: string;
  center: [number, number];
  scale: number;
}

export const datasets: Dataset[] = [
  {
    id: "census-blocks",
    name: "Census Blocks, Demographics",
    source: "United States Census",
    sourceUrl: "https://data.census.gov/",
    count: 11_155_486,
    url: "https://fd-stgeadlsv278968f2d-g4hrhshhhuaxgqc7.a02.azurefd.net/sop/0.1/census_blocks.parquet",
    center: [-74.006, 40.68],
    scale: 577_791 / 4,
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
    name: "Building Footprints, Japan",
    source: "OpenStreetMap contributors, Overture Maps Foundation",
    sourceUrl: "https://overturemaps.org/",
    count: 53_742_978,
    url: "https://fd-stgeadlsv278968f2d-g4hrhshhhuaxgqc7.a02.azurefd.net/sop/0.1/japan_buildings.parquet",
    bookmarks: [],
    center: [139.783372, 35.675360],
    scale: 144_448 / 4,
    // maxScale: 144_448 / 2, 
    byteSize: 6_177_950_661,
  },

  {
    id: "national-building-database-france",
    name: "National Building Database, France",
    source: "Centre Scientifique et Technique du Bâtiment",
    sourceUrl: "https://www.data.gouv.fr/datasets/base-de-donnees-nationale-des-batiments",
    count: 32_220_045,
    url: "https://fd-stgeadlsv278968f2d-g4hrhshhhuaxgqc7.a02.azurefd.net/sop/0.1/france_buildings.parquet",
    center: [2.3522, 48.8566],
    scale: 144_448 / 2,
    byteSize: 8_034_324_439,
  },

  {
    id: "alaska-3d-hydrography",
    name: "Alaska Hydrography",
    source: "U.S. Geological Survey",
    sourceUrl: "https://www.sciencebase.gov/catalog/item/69743a65d4be0260181a121b" ,
    count: 3_278_591,
    url: "https://fd-stgeadlsv278968f2d-g4hrhshhhuaxgqc7.a02.azurefd.net/sop/0.1/hydro_alaska.parquet",
    center: [-149.9003, 61.2181],
    scale: 144_448 * 16,
    basemap: "6178c4387d07481f87539022ab641aeb",
    spatialReference: 4326,
    byteSize: 1_967_053_038,
  },
  {
    id: "country-borders",
    name: "Country Borders, Overture",
    source: "OpenStreetMap contributors, Overture Maps Foundation",
    sourceUrl: "https://overturemaps.org/",
    count: 53_742_978,
    url: "https://fd-stgeadlsv278968f2d-g4hrhshhhuaxgqc7.a02.azurefd.net/sop/0.1/country_borders.parquet",
    bookmarks: [],
    center: [139.783372, 35.675360],
    scale: 144_448 / 4,
    // maxScale: 144_448 / 2, 
    byteSize: 6_177_950_661,
  },
]

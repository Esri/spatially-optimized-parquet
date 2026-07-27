
export type DatasetId =
  | "census-blocks"
  | "building-footprints-japan"
  | "national-building-database-france"
  | "alaska-3d-hydrography";

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
  zoom: number;
  byteSize: number;
}

export interface Bookmark {
  name: string;
  center: [number, number];
  zoom: number;
}

export const datasets: Dataset[] = [
  {
    id: "census-blocks",
    name: "Census Blocks, Demographics",
    source: "United States Census",
    count: 0,
    url: "https://fd-stgeadlsv278968f2d-g4hrhshhhuaxgqc7.a02.azurefd.net/sop/0.1/census_blocks.parquet",
    center: [-74.006, 40.68],
    zoom: 10,
    byteSize: 1024,
    bookmarks: [
      {
        name: "New York City, NY",
        center: [-74.006, 40.7128], 
        zoom: 12,
      },
      {
        name: "Washington, D.C.",
        center: [-77.03637, 38.89511],
        zoom: 12,
      },
      {
        name: "Dallas, TX",
        center: [-96.797, 32.7767],
        zoom: 12,
      },
      {
        name: "Los Angeles, CA",
        center: [-118.2437, 34.0522],
        zoom: 12,
      }
    ]
  },
  {
    id: "building-footprints-japan",
    name: "Building Footprints, Japan",
    source: "OpenStreetMap contributors, Overture Maps Foundation",
    sourceUrl: "https://overturemaps.org/",
    count: 0,
    url: "https://fd-stgeadlsv278968f2d-g4hrhshhhuaxgqc7.a02.azurefd.net/sop/0.1/japan_buildings.parquet",
    bookmarks: [],
    center: [139.6917, 35.6895],
    zoom: 12,
    byteSize: 1024,
  },

  {
    id: "national-building-database-france",
    name: "National Building Database, France",
    source: "Centre Scientifique et Technique du Bâtiment",
    sourceUrl: "https://www.data.gouv.fr/datasets/base-de-donnees-nationale-des-batiments",
    count: 0,
    url: "https://stgeadlsv278968f2d.blob.core.windows.net/parquet/baitment_groupe_compile.parquet",
    center: [2.3522, 48.8566],
    zoom: 12,
    byteSize: 1024,
  },

  {
    id: "alaska-3d-hydrography",
    name: "Alaska 3D Hydrography",
    source: "U.S. Geological Survey",
    sourceUrl: "https://www.sciencebase.gov/catalog/item/69743a65d4be0260181a121b" ,
    count: 0,
    url: "",
    center: [-74.006, 40.7128],
    zoom: 12,
    byteSize: 1024,
  },
]

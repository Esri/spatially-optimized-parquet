
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
  scale: number;
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
    count: 0,
    url: "https://fd-stgeadlsv278968f2d-g4hrhshhhuaxgqc7.a02.azurefd.net/sop/0.1/census_blocks.parquet",
    // url: "https://fd-stgeadlsv278968f2d-g4hrhshhhuaxgqc7.a02.azurefd.net/sop/0.1/census-blocks.parquet",
    center: [-74.006, 40.68],
    scale: 400000,
    byteSize: 1024,
    bookmarks: [
      {
        name: "New York City, NY",
        center: [-74.006, 40.7128], 
        scale: 100000,
      },
      {
        name: "Washington, D.C.",
        center: [-77.03637, 38.89511],
        scale: 100000,
      },
      {
        name: "Dallas, TX",
        center: [-96.797, 32.7767],
        scale: 100000,
      },
      {
        name: "Los Angeles, CA",
        center: [-118.2437, 34.0522],
        scale: 100000,
      }
    ]
  },
  {
    id: "building-footprints-japan",
    name: "Building Footprints, Japan",
    source: "OpenStreetMap contributors, Overture Maps Foundation",
    sourceUrl: "https://overturemaps.org/",
    count: 0,
    url: "",
    bookmarks: [],
    center: [-74.006, 40.7128],
    scale: 100000,
    byteSize: 1024,
  },

  {
    id: "national-building-database-france",
    name: "National Building Database, France",
    source: "Centre Scientifique et Technique du Bâtiment",
    sourceUrl: "https://www.data.gouv.fr/datasets/base-de-donnees-nationale-des-batiments",
    count: 0,
    url: "https://stgeadlsv278968f2d.blob.core.windows.net/parquet/baitment_groupe_compile.parquet",
    center: [-74.006, 40.7128],
    scale: 100000,
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
    scale: 100000,
    byteSize: 1024,
  },
]


## Spatially Optimized Parquet

This repository includes the [spec](spec/display-optimization.md) and reference implementation for Spatially Optimized Parquet.

Spatially Optimized Parquet (SOP) defines an additional layer of cluster-based spatial optimization on top of Apache Parquet, focused on client-oriented streaming of Parquet files for display. SOP allows a single copy of the data to be used for both analytical queries and display streaming, without the need for a separate set of pregenerated tiles. Because SOP is untiled, it maintains feature integrity, avoiding the issues that tile-clipping can introduce. Tile-clipping can negatively impact both client-side analytics and geometric, expression-based visualization. SOP uses spatial clustering to colocate nearby geographic features. Clients can leverage statistic-based predicate skipping through the use of a Page Index, to stream in the subset of the data needed for a given extent. For complex geometries, SOP also incudes several multiscale LOD columns.

Only two types of clustering are currently supported: xz for complex geometries, and z for points. Additional clustering types, especially to support spatiotemporal datasets, are planned for the future.

## Installation

First, [install Rust](https://rustup.rs/)

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

```

Then install with:

```sh
cargo install
```

## Usage

You can then run on a `geopackage` with:
```sh
parquet-opt
  --input france/gpkg/bdnb.gpkg \
  --layer batiment_groupe_compile \
  --output out.parquet \
  --output-files 1 \
  --overwrite
```

GeoParquet is also supported. Parquet without geospatial metadata can also be used provided the geometry column is tagged with `--geometry-column`. Add `--covering` to write a GeoParquet 1.1 root `bbox` covering column with `xmin`, `ymin`, `xmax`, and `ymax` fields.


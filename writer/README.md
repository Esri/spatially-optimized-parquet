# Getting started

First, [install Rust](https://rustup.rs/)

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

```

Then install with:

```sh
cargo install
```

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

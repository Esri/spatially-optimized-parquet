> [!NOTE]
> We are currently in the process of directly integrating Spatially Optimized Parquet into [GeoParquet](https://geoparquet.org/releases/v1.1.0/) as an optional DisplayOptimization extension.

## Spatially Optimized Parquet

This repository includes the in-progress [specification](spec/display-optimization.md) and reference implementation for Spatially Optimized Parquet.

Spatially Optimized Parquet (SOP) defines an additional layer of cluster-based spatial optimization on top of [Apache Parquet](https://parquet.apache.org/), focused on client-oriented streaming of Parquet files for display. SOP allows a single copy of the data to be used for both analytical queries and display streaming, without the need for a separate set of pregenerated tiles. Because SOP is untiled, it maintains feature integrity, avoiding the client-side analytical and visualization issues that tile-clipping can introduce. Instead, SOP uses spatial clustering [[Bohm et al. 1999]](http://dx.doi.org/10.1007/3-540-48482-5_7) to colocate nearby geographic features. Clients leverage statistic-based predicate skipping through the use of a Page Index, to stream in the subset of the data needed for a given extent. For complex geometries, SOP also includes several multiscale LOD columns.

Only two types of clustering are currently supported: xz for complex geometries, and z for points. Additional clustering types, especially to support spatiotemporal datasets, are planned for the future.

#### *2.6 Billion Building Footprints, Overture*
https://devtopia.esri.com/user-attachments/assets/256844e8-5480-45fc-921d-ef4e908b4977

#### *11 Million Census Blocks*
https://devtopia.esri.com/user-attachments/assets/495b42e7-0c61-4fde-8413-a9104aab94e4

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

Input format is inferred from `.gpkg` or `.parquet`. Local directories are treated as Parquet
datasets. Use `--input-format gpkg|parquet` for extensionless or unconventional locations.

Both output modes write GeoParquet:

- The default writes Spatially Optimized GeoParquet with spatial ordering and display columns.
- `--no-optimization` writes plain GeoParquet without SOP display columns or spatial sorting.

Both modes default to `--out-sr 4326`. When the source uses another CRS, the writer reprojects the
selected WKB rows and derives every output coordinate, extent, covering bbox, GeoParquet CRS/bbox,
and optimized geodisplay field from the WGS84 result. The current command intentionally panics
before opening input or mutating output for any `--out-sr` other than `4326`.

When Parquet geometry lacks CRS metadata, pass `--geometry-column <NAME> --in-sr <LATEST_WKID>`.
The writer scans WKB to infer geometry types and the selected-row extent. `--in-sr` fails when
the selected geometry already declares a CRS, preventing accidental overrides. `--covering`
works with both plain and optimized GeoParquet.

## Implementation layout

The Rust workspace separates source integration, format-neutral analysis, and output execution:

- `spatial::input::{gpkg, parquet}` owns format-specific discovery and scanning. Each source splits
  metadata, opening, and streaming concerns into focused modules.
- `spatial::analysis` derives geometry family, extent, dimensions, and CRS after both sources
  converge on the `InputSource` boundary.
- `spatial::job` stays thin. It validates resources and routes plain or optimized output.
- `spatial::geoparquet` owns source metadata normalization, GeoParquet JSON, covering behavior,
  and the plain GeoParquet workflow.
- `spatial::optimized` owns the optimized workflow, geodisplay metadata, spatial ordering, and
  multiscale geometry encoding.
- `spatial::output::reprojection` owns the shared CRS comparison, WKB transformation, point,
  bounds, and target-extent expressions used by both output modes.
- `spatial::output` retains shared writing, geometry-array, output-mode, spatial-reference, and
  reprojection mechanics.

See [architecture.md](architecture.md) for the complete execution flow and module map.

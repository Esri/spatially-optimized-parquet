> [!NOTE]
> We are currently working on proposal extensions to integrate Spatially Optimized Parquet into [GeoParquet](https://geoparquet.org/releases/v1.1.0/).

<img width="1428" height="590" alt="Screenshot 2026-08-26 at 4 30 35 PM" src="https://github.com/user-attachments/assets/ff50cfec-e5a8-4135-b854-c9d140f58a70" />

## Spatially Optimized Parquet

This repository includes the in-progress [specification](spec/display-optimization.md) and reference implementation for Spatially Optimized Parquet.

Spatially Optimized Parquet (SOP) defines an additional layer of cluster-based spatial optimization on top of [Apache Parquet](https://parquet.apache.org/), focused on client-oriented streaming of Parquet files for display. SOP allows a single copy of the data to be used for both analytical queries and display streaming, without the need for a separate set of pregenerated tiles. Because SOP is untiled, it maintains feature integrity, avoiding the client-side analytical and visualization issues that tile-clipping can introduce. Instead, SOP uses spatial clustering [[Bohm et al. 1999]](http://dx.doi.org/10.1007/3-540-48482-5_7) to colocate nearby geographic features. Clients leverage statistic-based predicate skipping through the use of a Page Index, to stream in the subset of the data needed for a given extent. For complex geometries, SOP also includes several multiscale LOD columns.

Only two types of clustering are currently supported: xz for complex geometries, and z for points. Additional clustering types, especially to support spatiotemporal datasets, are planned for the future.

#### *2.6 Billion Building Footprints, Overture* ([View](https://codepen.io/matt9222/pen/dPNVXZv))

https://github.com/user-attachments/assets/90f20705-ebf9-4c66-950c-d5c7877817f4


#### *11 Million Census Blocks* ([View](https://codepen.io/matt9222/pen/pvRWbpB))

https://github.com/user-attachments/assets/5c02b942-afb1-47cf-b892-27a5954b826c

## Installation

First, [setup Rust](https://rustup.rs/), then install with:

```sh
cargo install
```

## Usage

You can then run on a `geopackage` with:
```sh
sop write \
  france/bdnb.gpkg \
  --layer batiment_groupe_compile \
  --output out.parquet
```

Validate an existing optimized file or recursive partitioned dataset with:

```sh
sop validate <file>
```

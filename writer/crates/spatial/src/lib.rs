//! Implements the geospatial semantics of the Spatially Optimized Parquet writer.
//!
//! The crate follows one data-flow boundary. [`input`] providers normalize GeoPackage and
//! Parquet sources into Arrow schemas, metadata, batch streams, and DataFrames. [`analysis`]
//! resolves geometry category, extent, dimensions, and coordinate reference system. [`job`]
//! opens validated resources and routes execution into plain or optimized output workflows.
//!
//! Lower-level modules isolate the algorithms behind that flow. [`geoparquet`] owns the
//! GeoParquet product contract and plain workflow. [`optimized`] owns spatial clustering,
//! multiscale geometry encoding, geodisplay metadata, and optimized planning. [`output`] retains
//! mechanics shared by both products.

#![warn(missing_docs)]

pub mod analysis;
pub mod diagnostics;
pub mod geometry;
pub mod geoparquet;
pub mod input;
pub mod job;
pub mod optimized;
pub mod output;
pub mod progress;

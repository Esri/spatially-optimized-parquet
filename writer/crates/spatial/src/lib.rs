//! Implements the geospatial semantics of the Spatially Optimized Parquet writer.
//!
//! The crate follows one data-flow boundary. [`input`] providers normalize GeoPackage and
//! Parquet sources into Arrow schemas, metadata, batch streams, and DataFrames. [`geoparquet`]
//! resolves source geometry facts and the plain output contract. [`pipeline`] opens validated
//! resources and routes them through one explicit DataFusion execution path.
//!
//! Lower-level modules isolate the algorithms behind that flow. [`geoparquet`] owns the
//! GeoParquet product contract and plain output computations. [`optimized`] owns spatial clustering,
//! multiscale geometry encoding, geodisplay metadata, and optimized planning. [`output`] retains
//! mechanics shared by both products.

#![warn(missing_docs)]

pub mod diagnostics;
pub mod geometry;
pub mod geoparquet;
pub mod input;
pub mod optimized;
pub mod output;
pub mod pipeline;
pub mod progress;

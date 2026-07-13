//! Implements the geospatial semantics of the Spatially Optimized Parquet writer.
//!
//! The crate follows one data-flow boundary. [`input`] providers normalize GeoPackage and
//! Parquet sources into Arrow schemas, metadata, batch streams, and DataFrames. [`analysis`]
//! resolves geometry category, extent, dimensions, and coordinate reference system. [`job`]
//! opens validated resources and routes execution into plain or optimized output workflows.
//!
//! Lower-level modules isolate the algorithms behind that flow. [`output::optimized`] owns
//! spatial clustering, typed DataFusion expressions, reprojection, multiscale geometry encoding,
//! and optimized planning. [`metadata`] defines normalized source and serialized output models.

#![warn(missing_docs)]

pub mod analysis;
pub mod diagnostics;
pub mod geometry;
pub mod input;
pub mod job;
pub mod metadata;
pub mod output;
pub mod progress;

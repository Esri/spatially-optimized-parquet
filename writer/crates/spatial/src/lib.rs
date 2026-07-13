//! Implements the geospatial semantics of the Spatially Optimized Parquet writer.
//!
//! The crate follows one data-flow boundary. [`input`] providers normalize GeoPackage and
//! Parquet sources into Arrow schemas, metadata, batch streams, and DataFrames. [`analysis`]
//! resolves geometry category, extent, dimensions, and coordinate reference system. [`job`]
//! composes reprojection, spatial-code generation, multiscale encoding, sorting, metadata,
//! and Parquet writing into executable DataFusion plans.
//!
//! Lower-level modules isolate the algorithms behind that flow. [`codes`] implements Z/XZ
//! indexing, [`pbf`] flattens and quantizes geometry, [`udf`] exposes those operations to
//! DataFusion, [`reprojection`] wraps GDAL/PROJ transforms, and [`metadata`] defines the
//! normalized source and serialized output models. This separation keeps storage integration,
//! geometry computation, and execution orchestration independently testable.

#![warn(missing_docs)]

pub mod analysis;
pub mod codes;
pub mod display;
pub mod geometry;
pub mod input;
pub mod job;
pub mod metadata;
pub mod multiscale;
pub mod pbf;
pub mod reprojection;
pub mod udf;

//! Produces plain or spatially optimized GeoParquet from Parquet and GeoPackage sources.
//!
//! Configure one conversion through [`SpatialPipelineOptions`], which combines
//! [`InputOptions`] source policy with [`OutputOptions`] product policy, then submit it to
//! [`Pipeline`]. [`RowRange`] selects source rows, [`SourceFormat`] overrides source detection,
//! and [`OutputMode`] selects plain or optimized output.
//!
//! [`Pipeline::run`] returns [`SpatialPipelineResult`] after durable output completes. Attach a
//! [`WriteReporter`] when write progress matters. [`validate`] returns a [`ValidationReport`] for
//! an existing optimized file or partitioned dataset.

#![warn(missing_docs)]

mod diagnostics;
mod geometry;
mod geoparquet;
mod input;
mod optimized;
mod output;
mod pipeline;
mod session;
pub mod validate;

pub use geometry::GeometryError;
pub use geoparquet::DEFAULT_OUTPUT_WKID;
pub use geoparquet::GeoParquetError;
pub use input::InputError;
pub use input::{RowRange, SourceFormat};
pub use optimized::MultiscaleEncoding;
pub use output::OutputMode;
pub use pipeline::{
  InputOptions, OutputOptions, Pipeline, PipelineError, SpatialPipelineOptions,
  SpatialPipelineResult, WriteProgress, WriteReporter,
};
pub use session::SessionError;
pub use validate::{
  ValidationError, ValidationFailure, ValidationFinding, ValidationLocation, ValidationReport,
  ValidationRule, ValidationSeverity, validate,
};

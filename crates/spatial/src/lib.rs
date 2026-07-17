//! Writes and validates Spatially Optimized Parquet through stable public façades.
//!
//! Construct [`SpatialPipelineOptions`] from [`InputOptions`] and [`OutputOptions`], then pass the
//! request to [`Pipeline::run`]. [`RowRange`] selects source rows, [`SourceFormat`] overrides source
//! detection, and [`OutputMode`] chooses plain or optimized GeoParquet output. Attach a
//! [`WriteReporter`] when cumulative write counts are needed.
//!
//! [`Pipeline::run`] validates the request, executes the complete DataFusion workflow, writes
//! durable output, and automatically validates optimized output before returning
//! [`SpatialPipelineResult`].
//! [`validate`] inspects an existing file or recursive partitioned directory. The root façade keeps
//! storage adapters, geometry processing, optimization algorithms, and output mechanics private.

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

pub use geoparquet::DEFAULT_OUTPUT_WKID;
pub use input::{RowRange, SourceFormat};
pub use optimized::MultiscaleEncoding;
pub use output::OutputMode;
pub use pipeline::{
  InputOptions, OutputOptions, Pipeline, SpatialPipelineOptions, SpatialPipelineResult,
  WriteProgress, WriteReporter,
};
pub use validate::{
  ValidationFailure, ValidationFinding, ValidationLocation, ValidationReport, ValidationRule,
  ValidationSeverity, validate,
};

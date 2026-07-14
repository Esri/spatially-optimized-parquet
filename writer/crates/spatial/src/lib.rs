//! Writes and validates Spatially Optimized Parquet through stable public façades.
//!
//! Construct [`SpatialPipelineOptions`] from [`InputOptions`], [`OutputOptions`], and
//! [`ExecutionOptions`], then pass the request to [`run`]. [`RowRange`] selects source rows,
//! [`SourceFormat`] overrides source detection, and [`OutputMode`] chooses plain or optimized
//! GeoParquet output. [`DEFAULT_OUTPUT_WKID`] provides the default output spatial reference.
//!
//! [`run`] validates the request, executes the complete DataFusion workflow, writes durable output,
//! and automatically validates optimized output before returning [`SpatialPipelineResult`].
//! [`validate`] inspects an existing file or recursive partitioned directory. The root façade keeps
//! storage adapters, geometry processing, optimization algorithms, and output mechanics private.

#![warn(missing_docs)]

mod diagnostics;
mod geometry;
mod geoparquet;
mod input;
mod optimized;
mod output;
mod parquet_dataset;
mod pipeline;
mod progress;
mod session;
#[cfg(test)]
mod test_support;
pub mod validate;

pub use input::{RowRange, SourceFormat};
pub use output::{DEFAULT_OUTPUT_WKID, OutputMode};
pub use pipeline::{
  ExecutionOptions, InputOptions, OutputOptions, SpatialPipelineOptions, SpatialPipelineResult, run,
};
pub use validate::{
  ValidationFailure, ValidationFinding, ValidationLocation, ValidationReport, ValidationRule,
  ValidationSeverity, validate,
};

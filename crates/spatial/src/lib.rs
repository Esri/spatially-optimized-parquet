// Copyright 2026 Esri
//
// Licensed under the Apache License Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Produces plain or spatially optimized GeoParquet from datasets or owned Arrow batches.
//!
//! Configure one conversion through [`SpatialPipelineOptions`], which combines
//! [`InputOptions`] source policy with [`OutputOptions`] product policy, then submit it to
//! [`Pipeline`]. [`RowRange`] selects source rows, [`SourceFormat`] overrides source detection,
//! and [`OutputMode`] selects plain or optimized output.
//!
//! [`Pipeline::run`] returns [`SpatialPipelineResult`] after durable output completes. Attach a
//! [`WriteReporter`] when write progress matters. [`validate`] returns a [`ValidationReport`] for
//! an existing optimized file or partitioned dataset.
//!
//! [`PipelineSource::Dataset`] opens Parquet or GeoPackage input.
//! [`PipelineSource::Arrow`] accepts a shared Arrow schema, owned batches and a
//! [`GeometryDeclaration`], without writing an intermediate source file. Batches remain
//! in memory for repeated scans; this input is not a one-shot or bounded-memory stream.
//! Empty or all-null geometry retains its declared type and needs an explicit
//! [`OutputOptions::normalization_extent`]. Its output GeoParquet metadata omits the
//! unobserved bounding box.
//!
//! ```no_run
//! use std::{path::PathBuf, sync::Arc};
//! use arrow_array::{BinaryArray, RecordBatch};
//! use arrow_schema::{DataType, Field, Schema};
//! use spatial::{
//!   GeometryDeclaration, GeometryType, InputOptions, OutputOptions, Pipeline,
//!   PipelineSource, SpatialPipelineOptions,
//! };
//!
//! # async fn write_null_points() -> Result<(), spatial::PipelineError> {
//! let schema = Arc::new(Schema::new(vec![Field::new("geometry", DataType::Binary, true)]));
//! let batch = RecordBatch::new_empty(Arc::clone(&schema));
//! Pipeline::run(SpatialPipelineOptions {
//!   input: InputOptions {
//!     source: PipelineSource::Arrow {
//!       schema,
//!       batches: vec![batch],
//!       geometry: GeometryDeclaration::wkb("geometry", vec![GeometryType::Point], 4326)?,
//!     },
//!     ..InputOptions::default()
//!   },
//!   output: OutputOptions {
//!     path: PathBuf::from("points.parquet"),
//!     normalization_extent: Some([-180.0, -90.0, 180.0, 90.0]),
//!     row_group_size: Some(16_384),
//!     page_size: Some(1_048_576),
//!     ..OutputOptions::default()
//!   },
//!   ..SpatialPipelineOptions::default()
//! }).await?;
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]

mod diagnostics;
mod geometry;
mod geoparquet;
mod input;
mod optimized;
mod output;
mod pipeline;
mod serialization;
mod session;
pub mod validate;

pub use geometry::{CoordinateDimensions, GeometryError, GeometryType};
pub use geoparquet::DEFAULT_OUTPUT_WKID;
pub use geoparquet::GeoParquetError;
pub use input::InputError;
pub use input::{GeometryDeclaration, PipelineSource, RowRange, SourceFormat};
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

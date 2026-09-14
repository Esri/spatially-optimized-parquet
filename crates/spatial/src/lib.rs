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

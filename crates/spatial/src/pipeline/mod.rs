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

mod error;
mod extent_resolver;
mod geometry_scan;
mod normalized_spatial_frame;
mod pipeline;
mod progress;
mod reprojection;
mod resolved_spatial_source;
mod result;
mod spatial_write_context;
mod strip;
mod warnings;

pub use error::PipelineError;
pub(crate) use extent_resolver::ExtentResolver;
pub(crate) use normalized_spatial_frame::NormalizedSpatialFrame;
pub use pipeline::{InputOptions, OutputOptions, Pipeline, SpatialPipelineOptions};
pub(crate) use progress::SharedWriteReporter;
pub use progress::{WriteProgress, WriteReporter};
pub(crate) use reprojection::ResolvedReprojection;
pub(crate) use resolved_spatial_source::{ResolvedSpatialSource, resolve_source};
pub use result::SpatialPipelineResult;
pub(crate) use spatial_write_context::SpatialWriteContext;
pub(crate) use strip::StripGeometryDimensionsUdf;
pub(crate) use warnings::PipelineWarnings;

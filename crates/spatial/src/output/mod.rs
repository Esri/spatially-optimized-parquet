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

//! Provides mechanics shared by the GeoParquet output products.
//!
//! Product contracts and pipelines live in [`crate::geoparquet`] and [`crate::pipeline`].

mod error;
mod geometry_schema;
mod mode;
mod optimized;
mod partition_plan;
mod path;
mod plain;
mod reporter;
mod tracking_sink;
mod writer;

#[cfg(test)]
pub(crate) use crate::geometry::QuantizationTransform;
pub(crate) use error::OutputError;
pub use mode::OutputMode;
pub(crate) use optimized::{partitioned, single};
pub(crate) use path::OutputPath;
pub(crate) use plain::PlainWriter;
pub(crate) use writer::{Writer, WriterOptions};

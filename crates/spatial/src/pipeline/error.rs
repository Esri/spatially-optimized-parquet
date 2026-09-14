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

use crate::output::OutputError;
use crate::{GeoParquetError, GeometryError, InputError, SessionError};

/// Represents failures while converting one spatial dataset.
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
  /// Wraps an input-source failure.
  #[error(transparent)]
  Input(#[from] InputError),
  /// Wraps a geometry operation failure.
  #[error(transparent)]
  Geometry(#[from] GeometryError),
  /// Wraps a GeoParquet metadata or spatial-reference failure.
  #[error(transparent)]
  GeoParquet(#[from] GeoParquetError),
  /// Wraps a DataFusion session setup failure.
  #[error(transparent)]
  Session(#[from] SessionError),
  /// Wraps an output preparation or write failure.
  #[error(transparent)]
  Output(#[from] OutputError),
  /// Reports a DataFusion planning or execution failure.
  #[error("DataFusion pipeline operation {operation} failed: {source}")]
  DataFusion {
    /// Identifies the failed DataFusion operation.
    operation: &'static str,
    /// Preserves the DataFusion error.
    #[source]
    source: datafusion::common::DataFusionError,
  },
  /// Reports invalid pipeline request options.
  #[error("invalid pipeline request: {0}")]
  InvalidRequest(String),
}

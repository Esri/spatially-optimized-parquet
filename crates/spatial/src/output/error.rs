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

use std::path::PathBuf;

/// Represents failures while preparing or writing output.
#[derive(Debug, thiserror::Error)]
pub enum OutputError {
  /// Reports GeoParquet metadata construction failures during output preparation.
  #[error(transparent)]
  GeoParquet(#[from] crate::GeoParquetError),
  /// Reports output path-policy failures.
  #[error("output path error: {0}")]
  Path(String),
  /// Reports invalid writer configuration.
  #[error("output configuration error: {0}")]
  Configuration(String),
  /// Reports a filesystem operation failure.
  #[error("output filesystem operation {operation} failed for {}: {source}", path.display())]
  Io {
    /// Identifies the failed filesystem operation.
    operation: &'static str,
    /// Identifies the affected output path.
    path: PathBuf,
    /// Preserves the filesystem error.
    #[source]
    source: std::io::Error,
  },
  /// Reports a Parquet writer failure.
  #[error("Parquet output operation {operation} failed: {source}")]
  Parquet {
    /// Identifies the failed Parquet operation.
    operation: &'static str,
    /// Preserves the Parquet error.
    #[source]
    source: parquet::errors::ParquetError,
  },
  /// Reports a DataFusion output operation failure.
  #[error("DataFusion output operation {operation} failed: {source}")]
  DataFusion {
    /// Identifies the failed DataFusion operation.
    operation: &'static str,
    /// Preserves the DataFusion error.
    #[source]
    source: datafusion::common::DataFusionError,
  },
}

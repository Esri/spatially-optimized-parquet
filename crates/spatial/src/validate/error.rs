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

use crate::{GeometryError, InputError};

/// Represents operational failures while validating one SOP dataset.
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
  /// Wraps input discovery failures.
  #[error(transparent)]
  Input(#[from] InputError),
  /// Wraps sampled WKB geometry failures.
  #[error(transparent)]
  Geometry(#[from] GeometryError),
  /// Reports a Parquet footer or reader failure.
  #[error("validation Parquet operation failed for {}: {source}", path.display())]
  Parquet {
    /// Identifies the affected file.
    path: PathBuf,
    /// Preserves the Parquet error.
    #[source]
    source: parquet::errors::ParquetError,
  },
  /// Reports an invalid validation location.
  #[error("validation path must be a .parquet file or directory: {}", path.display())]
  InvalidPath {
    /// Identifies the invalid path.
    path: PathBuf,
  },
  /// Reports an invalid Arrow column path or type.
  #[error("{0}")]
  ArrowColumn(String),
  /// Reports a validation filesystem operation failure.
  #[error("{operation}: {}: {source}", path.display())]
  Io {
    /// Identifies the failed filesystem operation.
    operation: &'static str,
    /// Identifies the affected file.
    path: PathBuf,
    /// Preserves the filesystem error.
    #[source]
    source: std::io::Error,
  },
  /// Reports an Arrow row-reading failure.
  #[error("{operation}: {}: {source}", path.display())]
  Arrow {
    /// Identifies the failed Arrow operation.
    operation: &'static str,
    /// Identifies the affected file.
    path: PathBuf,
    /// Preserves the Arrow error.
    #[source]
    source: arrow_schema::ArrowError,
  },
}

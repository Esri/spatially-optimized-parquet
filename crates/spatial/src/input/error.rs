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

/// Represents failures while identifying, opening, or reading an input source.
#[derive(Debug, thiserror::Error)]
pub enum InputError {
  /// Reports an unsupported or invalid source format.
  #[error("invalid input format: {0}")]
  Format(String),
  /// Reports a filesystem operation failure.
  #[error("input filesystem operation {operation} failed for {}: {source}", path.display())]
  Path {
    /// Identifies the failed filesystem operation.
    operation: &'static str,
    /// Identifies the affected input path.
    path: PathBuf,
    /// Preserves the underlying filesystem error.
    #[source]
    source: std::io::Error,
  },
  /// Reports a Parquet metadata or scan operation failure.
  #[error("Parquet input operation {operation} failed: {source}")]
  Parquet {
    /// Identifies the failed Parquet operation.
    operation: &'static str,
    /// Preserves the Parquet error.
    #[source]
    source: parquet::errors::ParquetError,
  },
  /// Reports a GeoPackage provider operation failure.
  #[error("GeoPackage input operation {operation} failed: {source}")]
  GeoPackage {
    /// Identifies the failed GeoPackage operation.
    operation: &'static str,
    /// Preserves the GDAL error.
    #[source]
    source: gdal::errors::GdalError,
  },
  /// Reports a remote object-store operation failure.
  #[error("object-store operation {operation} failed: {source}")]
  ObjectStore {
    /// Identifies the failed object-store operation.
    operation: &'static str,
    /// Preserves the object-store error.
    #[source]
    source: object_store::Error,
  },
  /// Reports malformed GeoParquet metadata or incompatible source files.
  #[error("input metadata error: {0}")]
  Metadata(String),
  /// Reports an invalid numeric value in input metadata.
  #[error("input metadata integer parse failed for {value}: {source}")]
  ParseInteger {
    /// Identifies the source value.
    value: String,
    /// Preserves the numeric parse failure.
    #[source]
    source: std::num::ParseIntError,
  },
  /// Reports a malformed input URL.
  #[error("invalid input URL: {source}")]
  Url {
    /// Preserves the URL parse failure.
    #[source]
    source: url::ParseError,
  },
  /// Reports JSON metadata decoding failures.
  #[error("input metadata JSON operation {operation} failed: {source}")]
  Json {
    /// Identifies the failed JSON operation.
    operation: &'static str,
    /// Preserves the JSON failure.
    #[source]
    source: serde_json::Error,
  },
  /// Reports a DataFusion input operation failure.
  #[error("DataFusion input operation {operation} failed: {source}")]
  DataFusion {
    /// Identifies the failed DataFusion operation.
    operation: &'static str,
    /// Preserves the DataFusion error.
    #[source]
    source: datafusion::common::DataFusionError,
  },
  /// Reports an Arrow stream or record-batch operation failure.
  #[error("Arrow input operation {operation} failed: {source}")]
  Arrow {
    /// Identifies the failed Arrow operation.
    operation: &'static str,
    /// Preserves the Arrow failure.
    #[source]
    source: arrow_schema::ArrowError,
  },
}

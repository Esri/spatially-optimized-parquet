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

/// Represents failures while resolving or serializing GeoParquet metadata.
#[derive(Debug, thiserror::Error)]
pub enum GeoParquetError {
  /// Reports missing or invalid GeoParquet metadata.
  #[error("GeoParquet metadata error: {0}")]
  Metadata(String),
  /// Reports an output coordinate reference system the writer does not support.
  #[error("unsupported output spatial reference EPSG:{wkid}; expected EPSG:4326 or EPSG:3857")]
  UnsupportedOutputSpatialReference {
    /// Identifies the rejected EPSG well-known identifier.
    wkid: u32,
  },
  /// Reports a coordinate-reference operation failure.
  #[error("spatial reference operation {operation} failed: {source}")]
  SpatialReference {
    /// Identifies the failed spatial-reference operation.
    operation: &'static str,
    /// Preserves the GDAL failure.
    #[source]
    source: gdal::errors::GdalError,
  },
  /// Reports JSON serialization or deserialization failures.
  #[error("GeoParquet JSON operation {operation} failed: {source}")]
  Json {
    /// Identifies the failed JSON operation.
    operation: &'static str,
    /// Preserves the JSON failure.
    #[source]
    source: serde_json::Error,
  },
}

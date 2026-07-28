/// Represents failures while resolving or serializing GeoParquet metadata.
#[derive(Debug, thiserror::Error)]
pub enum GeoParquetError {
  /// Reports missing or invalid GeoParquet metadata.
  #[error("GeoParquet metadata error: {0}")]
  Metadata(String),
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

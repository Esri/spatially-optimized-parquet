/// Represents failures while parsing, transforming, or encoding geometry values.
#[derive(Debug, thiserror::Error)]
pub enum GeometryError {
  /// Reports malformed or unsupported WKB input.
  #[error("WKB error: {0}")]
  Wkb(String),
  /// Reports invalid geometry topology or dimensions.
  #[error("invalid geometry: {0}")]
  InvalidGeometry(String),
  /// Reports quantization failures.
  #[error("geometry quantization failed: {0}")]
  Quantization(String),
  /// Reports PBF encoding failures.
  #[error("PBF encoding failed: {0}")]
  Pbf(String),
  /// Reports Arrow array construction failures.
  #[error("Arrow geometry operation failed: {0}")]
  Arrow(String),
}

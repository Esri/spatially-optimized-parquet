use parquet::file::metadata::KeyValue;
use serde_json::Value;

use crate::analysis::Extent2D;
use crate::geometry::{GeometryEncoding, GeometryKind};

#[derive(Debug, Clone, PartialEq)]
pub struct SourceGeometryMetadata {
  pub column: String,
  pub encoding: GeometryEncoding,
  pub geometry_types: Vec<GeometryKind>,
  pub bbox: Option<Extent2D>,
  pub projjson: Option<Value>,
  pub has_z: bool,
  pub has_m: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SourceDatasetMetadata {
  pub geometry: Option<SourceGeometryMetadata>,
  pub passthrough_kv: Vec<KeyValue>,
}

//! Stores format-neutral metadata normalized by input providers.

use parquet::file::metadata::KeyValue;
use serde_json::Value;

use crate::geometry::{Extent2D, GeometryEncoding, GeometryKind};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourceGeometryMetadata {
  pub(crate) column: String,
  pub(crate) encoding: GeometryEncoding,
  pub(crate) geometry_types: Vec<GeometryKind>,
  pub(crate) bbox: Option<Extent2D>,
  pub(crate) projjson: Option<Value>,
  pub(crate) has_z: bool,
  pub(crate) has_m: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SourceDatasetMetadata {
  pub(crate) geometry: Option<SourceGeometryMetadata>,
  pub(crate) passthrough_kv: Vec<KeyValue>,
}

//! Stores format-neutral metadata normalized by input providers.

use parquet::file::metadata::KeyValue;
use serde_json::Value;

use crate::geometry::{Extent2D, GeometryEncoding, GeometryKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceCoveringMetadata {
  pub(crate) column: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourcePointOptimizationMetadata {
  pub(crate) code: String,
  pub(crate) x_column: String,
  pub(crate) y_column: String,
  pub(crate) coordinate_precision: u32,
  pub(crate) full_extent: Extent2D,
  pub(crate) wkid: Option<u32>,
  pub(crate) wkt: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourceGeometryMetadata {
  pub(crate) column: String,
  pub(crate) encoding: GeometryEncoding,
  pub(crate) geometry_types: Vec<GeometryKind>,
  pub(crate) bbox: Option<Extent2D>,
  pub(crate) covering: Option<SourceCoveringMetadata>,
  pub(crate) projjson: Option<Value>,
  pub(crate) has_z: bool,
  pub(crate) has_m: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SourceDatasetMetadata {
  pub(crate) geometry: Option<SourceGeometryMetadata>,
  pub(crate) point_optimization: Option<SourcePointOptimizationMetadata>,
  pub(crate) passthrough_kv: Vec<KeyValue>,
}

//! Stores format-neutral metadata discovered before DataFusion executes source rows.
//!
//! Providers translate GDAL layer properties or GeoParquet footer JSON into
//! [`SourceGeometryMetadata`]. The model carries enough information for geometry-column
//! inference, metadata-only analysis, CRS planning, dimension validation, and reconstruction
//! of output GeoParquet metadata.
//!
//! [`SourceDatasetMetadata::passthrough_kv`] contains only keys that the writer may safely
//! preserve. Providers exclude reserved schema and spatial keys because the optimization job
//! must regenerate those values to describe transformed output accurately.

use parquet::file::metadata::KeyValue;
use serde_json::Value;

use crate::geometry::Extent2D;
use crate::geometry::{GeometryEncoding, GeometryKind};

#[derive(Debug, Clone, PartialEq)]
/// Describes one source geometry column after provider-specific metadata normalization.
pub struct SourceGeometryMetadata {
  /// Names the source geometry column.
  pub column: String,
  /// Identifies the geometry column's physical encoding.
  pub encoding: GeometryEncoding,
  /// Lists every geometry kind declared or observed in the source.
  pub geometry_types: Vec<GeometryKind>,
  /// Stores the source-wide extent when metadata supplies one.
  pub bbox: Option<Extent2D>,
  /// Stores the coordinate reference system as PROJJSON.
  pub projjson: Option<Value>,
  /// Indicates whether source coordinates include a Z ordinate.
  pub has_z: bool,
  /// Indicates whether source coordinates include an M ordinate.
  pub has_m: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
/// Collects normalized spatial metadata and safe file metadata for output propagation.
pub struct SourceDatasetMetadata {
  /// Stores metadata for the selected geometry column.
  pub geometry: Option<SourceGeometryMetadata>,
  /// Stores non-reserved Parquet key-value metadata that may pass through to output.
  pub passthrough_kv: Vec<KeyValue>,
}

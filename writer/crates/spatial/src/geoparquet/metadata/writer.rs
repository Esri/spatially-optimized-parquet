//! Builds the GeoParquet JSON contract and Parquet key-value metadata.

use anyhow::{Context, Result};
use parquet::file::metadata::KeyValue;
use serde_json::Value;

use crate::analysis::{Extent2D, SpatialReferenceInfo};
use crate::geometry::GeometryKind;

use super::source::SourceDatasetMetadata;

/// Build GeoParquet 1.1 metadata for one geometry column and optional covering bbox.
pub fn build_geo_metadata(
  geometry_column: &str,
  geometry_types: &[GeometryKind],
  output_extent: Extent2D,
  output_spatial_reference: &SpatialReferenceInfo,
  has_z: bool,
  has_m: bool,
  covering: bool,
  covering_column: &str,
) -> Result<String> {
  let mut column = serde_json::Map::new();
  column.insert("encoding".to_string(), Value::String("WKB".to_string()));
  column.insert(
    "geometry_types".to_string(),
    Value::Array(
      geometry_types
        .iter()
        .copied()
        .map(|geometry_kind| {
          Ok(Value::String(geoparquet_geometry_type_name(
            geometry_kind,
            has_z,
            has_m,
          )?))
        })
        .collect::<Result<Vec<_>>>()?,
    ),
  );
  column.insert(
    "bbox".to_string(),
    serde_json::json!([
      output_extent.xmin,
      output_extent.ymin,
      output_extent.xmax,
      output_extent.ymax
    ]),
  );
  column.insert(
    "crs".to_string(),
    output_spatial_reference
      .projjson
      .clone()
      .context("missing output CRS PROJJSON")?,
  );
  if covering {
    column.insert(
      "covering".to_string(),
      geo_covering_bbox_metadata(covering_column),
    );
  }

  let mut columns = serde_json::Map::new();
  columns.insert(geometry_column.to_string(), Value::Object(column));
  Ok(serde_json::to_string(&serde_json::json!({
    "version": "1.1.0",
    "primary_column": geometry_column,
    "columns": columns,
  }))?)
}

/// Replace reserved GeoParquet metadata while preserving safe source key-value entries.
pub fn build_geo_key_values(
  source_metadata: &SourceDatasetMetadata,
  geo_metadata: String,
) -> Vec<KeyValue> {
  let mut metadata = source_metadata.passthrough_kv.clone();
  metadata.retain(|item| item.key != "geo");
  metadata.push(KeyValue {
    key: "geo".to_string(),
    value: Some(geo_metadata),
  });
  metadata
}

fn geo_covering_bbox_metadata(covering_column: &str) -> Value {
  serde_json::json!({
    "bbox": {
      "xmin": [covering_column, "xmin"],
      "ymin": [covering_column, "ymin"],
      "xmax": [covering_column, "xmax"],
      "ymax": [covering_column, "ymax"],
    }
  })
}

fn geoparquet_geometry_type_name(
  geometry_kind: GeometryKind,
  has_z: bool,
  has_m: bool,
) -> Result<String> {
  let base = match geometry_kind {
    GeometryKind::Point => "Point",
    GeometryKind::LineString => "LineString",
    GeometryKind::MultiPoint => "MultiPoint",
    GeometryKind::MultiLineString => "MultiLineString",
    GeometryKind::Polygon => "Polygon",
    GeometryKind::MultiPolygon => "MultiPolygon",
    GeometryKind::GeometryCollection => "GeometryCollection",
    GeometryKind::Unknown => return Err(anyhow::anyhow!("unsupported geometry kind metadata")),
  };
  let suffix = match (has_z, has_m) {
    (false, false) => "",
    (true, false) => " Z",
    (false, true) => " M",
    (true, true) => " ZM",
  };
  Ok(format!("{base}{suffix}"))
}

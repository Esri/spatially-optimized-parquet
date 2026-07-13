use std::collections::BTreeSet;

use anyhow::{Context, Result};
use geoparquet::metadata::{GeoParquetColumnEncoding, GeoParquetGeometryType, GeoParquetMetadata};
use parquet::arrow::arrow_reader::ArrowReaderMetadata;
use parquet::file::metadata::KeyValue;
use serde_json::Value;

use crate::analysis::Extent2D;
use crate::geometry::{GeometryEncoding, GeometryKind};
use crate::metadata::source::SourceGeometryMetadata;

/// Parse and require consistent GeoParquet metadata across all discovered files.
pub(super) fn load_geo_metadata(
  metadata_items: &[ArrowReaderMetadata],
) -> Result<Option<GeoParquetMetadata>> {
  let mut geo_meta: Option<GeoParquetMetadata> = None;
  let mut saw_geo = false;
  let mut saw_missing_geo = false;

  for metadata in metadata_items {
    match parse_geo_metadata(metadata).context("parse geoparquet metadata")? {
      Some(file_geo_meta) => {
        saw_geo = true;
        if let Some(existing) = geo_meta.as_mut() {
          existing.try_update(&file_geo_meta)?;
        } else {
          geo_meta = Some(file_geo_meta);
        }
      }
      None => saw_missing_geo = true,
    }
  }

  if saw_geo && saw_missing_geo {
    return Err(anyhow::anyhow!(
      "inconsistent geoparquet metadata across input files"
    ));
  }

  Ok(geo_meta)
}

/// Decode the GeoParquet `geo` key from one Parquet footer.
fn parse_geo_metadata(metadata: &ArrowReaderMetadata) -> Result<Option<GeoParquetMetadata>> {
  let Some(geo_value) = metadata
    .metadata()
    .file_metadata()
    .key_value_metadata()
    .and_then(|items| items.iter().find(|item| item.key == "geo"))
    .and_then(|item| item.value.as_ref())
  else {
    return Ok(None);
  };

  let mut json: Value = serde_json::from_str(geo_value).context("decode geo metadata json")?;
  sanitize_geo_metadata_json(&mut json);
  serde_json::from_value(json)
    .context("deserialize geo metadata")
    .map(Some)
}

/// Remove non-semantic metadata differences before comparing file-level GeoParquet JSON.
fn sanitize_geo_metadata_json(json: &mut Value) {
  let Some(columns) = json.get_mut("columns").and_then(Value::as_object_mut) else {
    return;
  };

  for column_meta in columns.values_mut() {
    let Some(object) = column_meta.as_object_mut() else {
      continue;
    };

    let remove_bbox = object
      .get("bbox")
      .and_then(Value::as_array)
      .is_some_and(|bbox| bbox.iter().any(Value::is_null));
    if remove_bbox {
      object.remove("bbox");
    }
  }
}

/// Convert GeoParquet metadata into the format-neutral source geometry model.
pub(super) fn build_source_geometry_metadata(
  geo_meta: &GeoParquetMetadata,
) -> Result<Option<SourceGeometryMetadata>> {
  let Some(column_meta) = geo_meta.columns.get(&geo_meta.primary_column) else {
    return Ok(None);
  };
  if column_meta.encoding != GeoParquetColumnEncoding::WKB {
    return Ok(None);
  }

  let geometry_types = column_meta
    .geometry_types
    .iter()
    .map(|geometry_type| map_geo_geometry_type(geometry_type.geometry_type()))
    .collect();

  Ok(Some(SourceGeometryMetadata {
    column: geo_meta.primary_column.clone(),
    encoding: GeometryEncoding::Wkb,
    geometry_types,
    bbox: bbox_to_extent(column_meta.bbox.as_deref()),
    projjson: column_meta.crs.clone(),
    has_z: has_dimension_suffix(column_meta, "Z"),
    has_m: has_dimension_suffix(column_meta, "M"),
  }))
}

pub(super) fn map_geo_geometry_type(geometry_type: GeoParquetGeometryType) -> GeometryKind {
  match geometry_type {
    GeoParquetGeometryType::Point => GeometryKind::Point,
    GeoParquetGeometryType::LineString => GeometryKind::LineString,
    GeoParquetGeometryType::MultiPoint => GeometryKind::MultiPoint,
    GeoParquetGeometryType::MultiLineString => GeometryKind::MultiLineString,
    GeoParquetGeometryType::Polygon => GeometryKind::Polygon,
    GeoParquetGeometryType::MultiPolygon => GeometryKind::MultiPolygon,
    GeoParquetGeometryType::GeometryCollection => GeometryKind::GeometryCollection,
  }
}

fn bbox_to_extent(bbox: Option<&[f64]>) -> Option<Extent2D> {
  let bbox = bbox?;
  if bbox.len() < 4 {
    return None;
  }
  Some(Extent2D {
    xmin: bbox[0],
    ymin: bbox[1],
    xmax: bbox[bbox.len() - 2],
    ymax: bbox[bbox.len() - 1],
  })
}

fn has_dimension_suffix(
  column_meta: &geoparquet::metadata::GeoParquetColumnMetadata,
  dimension: &str,
) -> bool {
  column_meta.geometry_types.iter().any(|geometry_type| {
    let value = geometry_type.to_string();
    match dimension {
      "Z" => value.ends_with(" Z") || value.ends_with(" ZM"),
      "M" => value.ends_with(" M") || value.ends_with(" ZM"),
      _ => false,
    }
  })
}

/// Preserve non-reserved key-value metadata exactly once across a file set.
pub(super) fn passthrough_metadata(metadata_items: &[ArrowReaderMetadata]) -> Vec<KeyValue> {
  let mut seen = BTreeSet::new();
  let mut out = Vec::new();
  for metadata in metadata_items {
    if let Some(kv_metadata) = metadata.metadata().file_metadata().key_value_metadata() {
      for kv in kv_metadata {
        if kv.key == "geo" || kv.key == "geodisplay" || kv.key == "ARROW:schema" {
          continue;
        }
        let identity = (kv.key.clone(), kv.value.clone());
        if seen.insert(identity) {
          out.push(kv.clone());
        }
      }
    }
  }
  out
}

/// Collect all file metadata needed by callers, including reserved keys.
pub(super) fn file_metadata(metadata_items: &[ArrowReaderMetadata]) -> Vec<KeyValue> {
  let mut seen = BTreeSet::new();
  let mut out = Vec::new();
  for metadata in metadata_items {
    if let Some(kv_metadata) = metadata.metadata().file_metadata().key_value_metadata() {
      for kv in kv_metadata {
        if kv.key == "ARROW:schema" {
          continue;
        }
        let identity = (kv.key.clone(), kv.value.clone());
        if seen.insert(identity) {
          out.push(kv.clone());
        }
      }
    }
  }
  out
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::{bbox_to_extent, sanitize_geo_metadata_json};

  #[test]
  fn metadata_sanitization_removes_null_bbox_values() {
    let mut metadata = json!({
      "columns": {
        "geometry": {
          "encoding": "WKB",
          "bbox": [null, null, null, null]
        }
      }
    });

    sanitize_geo_metadata_json(&mut metadata);

    assert!(metadata["columns"]["geometry"].get("bbox").is_none());
  }

  #[test]
  fn metadata_sanitization_preserves_complete_bbox_values() {
    let mut metadata = json!({
      "columns": {
        "geometry": {
          "encoding": "WKB",
          "bbox": [-1.0, -2.0, 3.0, 4.0]
        }
      }
    });

    sanitize_geo_metadata_json(&mut metadata);

    assert_eq!(
      metadata["columns"]["geometry"]["bbox"],
      json!([-1.0, -2.0, 3.0, 4.0])
    );
  }

  #[test]
  fn bbox_normalization_uses_outer_xy_values_for_dimensioned_bounds() {
    let extent = bbox_to_extent(Some(&[-1.0, -2.0, 10.0, 20.0, 3.0, 4.0])).unwrap();

    assert_eq!(extent.xmin, -1.0);
    assert_eq!(extent.ymin, -2.0);
    assert_eq!(extent.xmax, 3.0);
    assert_eq!(extent.ymax, 4.0);
  }

  #[test]
  fn bbox_normalization_rejects_incomplete_bounds() {
    assert!(bbox_to_extent(Some(&[-1.0, -2.0, 3.0])).is_none());
    assert!(bbox_to_extent(None).is_none());
  }
}

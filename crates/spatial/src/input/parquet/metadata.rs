use std::collections::BTreeSet;

use anyhow::{Context, Result};
use geoparquet::metadata::{GeoParquetColumnEncoding, GeoParquetGeometryType, GeoParquetMetadata};
use parquet::arrow::arrow_reader::ArrowReaderMetadata;
use parquet::file::metadata::KeyValue;
use serde_json::Value;

use crate::geometry::Extent2D;
use crate::geometry::{GeometryEncoding, GeometryKind};
use crate::input::{SourceCoveringMetadata, SourceGeometryMetadata};

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
  let Some(mut json) = metadata_json(metadata, "geo")? else {
    return Ok(None);
  };
  sanitize_geo_metadata_json(&mut json);
  serde_json::from_value(json)
    .context("deserialize geo metadata")
    .map(Some)
}

fn metadata_json(metadata: &ArrowReaderMetadata, key: &str) -> Result<Option<Value>> {
  let Some(value) = metadata
    .metadata()
    .file_metadata()
    .key_value_metadata()
    .and_then(|items| items.iter().find(|item| item.key == key))
    .and_then(|item| item.value.as_ref())
  else {
    return Ok(None);
  };
  serde_json::from_str(value)
    .with_context(|| format!("decode {key} metadata json"))
    .map(Some)
}

pub(super) fn load_covering_metadata(
  metadata_items: &[ArrowReaderMetadata],
  geometry_column: &str,
) -> Result<Option<SourceCoveringMetadata>> {
  let mut covering = None;
  let mut saw_missing = false;
  for metadata in metadata_items {
    let file_covering = metadata_json(metadata, "geo")?
      .as_ref()
      .and_then(|json| covering_column(json, geometry_column));
    match file_covering {
      Some(file_covering) => {
        if saw_missing
          || covering
            .as_ref()
            .is_some_and(|existing| existing != &file_covering)
        {
          return Ok(None);
        }
        covering = Some(file_covering);
      }
      None => {
        if covering.is_some() {
          return Ok(None);
        }
        saw_missing = true;
      }
    }
  }
  Ok(covering)
}

fn covering_column(json: &Value, geometry_column: &str) -> Option<SourceCoveringMetadata> {
  let bbox = json
    .get("columns")?
    .get(geometry_column)?
    .get("covering")?
    .get("bbox")?;
  let paths = [
    ("xmin", bbox.get("xmin")?),
    ("ymin", bbox.get("ymin")?),
    ("xmax", bbox.get("xmax")?),
    ("ymax", bbox.get("ymax")?),
  ];
  let mut covering_column = None;
  for (expected_field, path) in paths {
    let path = path.as_array()?;
    if path.len() != 2 || path[1].as_str()? != expected_field {
      return None;
    }
    let column = path[0].as_str()?;
    match &covering_column {
      Some(existing) if existing != column => return None,
      Some(_) => {}
      None => covering_column = Some(column.to_string()),
    }
  }
  covering_column.map(|column| SourceCoveringMetadata { column })
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

impl SourceGeometryMetadata {
  /// Construct normalized source geometry metadata from a GeoParquet contract.
  pub(super) fn from_geoparquet(
    geo_meta: &GeoParquetMetadata,
    covering: Option<SourceCoveringMetadata>,
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
      covering,
      projjson: column_meta.crs.clone(),
      has_z: has_dimension_suffix(column_meta, "Z"),
      has_m: has_dimension_suffix(column_meta, "M"),
    }))
  }
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

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::{bbox_to_extent, covering_column, has_dimension_suffix, sanitize_geo_metadata_json};

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

  #[test]
  fn geoparquet_geometry_type_inference_preserves_z_and_m_flags() {
    for (geometry_type, expected_z, expected_m) in [
      ("Point Z", true, false),
      ("Point M", false, true),
      ("Point ZM", true, true),
    ] {
      let column: geoparquet::metadata::GeoParquetColumnMetadata = serde_json::from_value(json!({
        "encoding": "WKB",
        "geometry_types": [geometry_type],
        "crs": null,
        "orientation": "counterclockwise",
        "edges": "planar"
      }))
      .unwrap();

      assert_eq!(has_dimension_suffix(&column, "Z"), expected_z);
      assert_eq!(has_dimension_suffix(&column, "M"), expected_m);
    }
  }

  #[test]
  fn covering_normalization_accepts_one_root_bbox_struct() {
    let metadata = json!({
      "columns": {
        "geometry": {
          "covering": {
            "bbox": {
              "xmin": ["source_bbox", "xmin"],
              "ymin": ["source_bbox", "ymin"],
              "xmax": ["source_bbox", "xmax"],
              "ymax": ["source_bbox", "ymax"]
            }
          }
        }
      }
    });

    assert_eq!(
      covering_column(&metadata, "geometry").unwrap().column,
      "source_bbox"
    );
  }

  #[test]
  fn covering_normalization_rejects_mixed_or_deep_paths() {
    let mixed = json!({
      "columns": {
        "geometry": {
          "covering": {
            "bbox": {
              "xmin": ["bbox", "xmin"],
              "ymin": ["other", "ymin"],
              "xmax": ["bbox", "xmax"],
              "ymax": ["bbox", "ymax"]
            }
          }
        }
      }
    });
    let deep = json!({
      "columns": {
        "geometry": {
          "covering": {
            "bbox": {
              "xmin": ["geodisplay", "bounds", "xmin"],
              "ymin": ["geodisplay", "bounds", "ymin"],
              "xmax": ["geodisplay", "bounds", "xmax"],
              "ymax": ["geodisplay", "bounds", "ymax"]
            }
          }
        }
      }
    });

    assert!(covering_column(&mixed, "geometry").is_none());
    assert!(covering_column(&deep, "geometry").is_none());
  }
}

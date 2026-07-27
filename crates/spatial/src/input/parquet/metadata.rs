use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail};
use gdal::spatial_ref::SpatialRef;
use parquet::arrow::arrow_reader::ArrowReaderMetadata;
use parquet::file::metadata::KeyValue;
use serde::Deserialize;
use serde_json::Value;

use crate::geometry::Extent2D;
use crate::geometry::{GeometryEncoding, GeometryKind};
use crate::input::{SourceCoveringMetadata, SourceGeometryMetadata};

use super::source::ParquetInputSource;

/// Represents the GeoParquet input fields consumed by the spatial pipeline.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(super) struct ParquetGeoMetadata {
  pub(super) primary_column: String,
  columns: BTreeMap<String, ParquetGeoColumnMetadata>,
}

/// Represents one input geometry column without owning the output metadata contract.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(super) struct ParquetGeoColumnMetadata {
  encoding: String,
  #[serde(default)]
  geometry_types: Vec<String>,
  #[serde(default)]
  bbox: Option<Vec<Option<f64>>>,
  #[serde(default)]
  crs: Option<Value>,
}

/// Represents one geometry type parsed from GeoParquet metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ParsedGeometryType {
  pub(super) kind: GeometryKind,
  has_z: bool,
  has_m: bool,
}

impl ParquetGeoMetadata {
  pub(super) fn primary_geometry(&self) -> Option<&ParquetGeoColumnMetadata> {
    self.columns.get(&self.primary_column)
  }

  fn merge(&mut self, other: Self) -> Result<()> {
    if self.primary_column != other.primary_column {
      bail!(
        "inconsistent geoparquet primary column across input files: {} != {}",
        self.primary_column,
        other.primary_column
      );
    }

    let primary_column = self.primary_column.clone();
    let Some(current) = self.columns.get_mut(&primary_column) else {
      bail!("geoparquet primary column is missing metadata: {primary_column}");
    };
    let Some(incoming) = other.columns.get(&primary_column) else {
      bail!("geoparquet primary column is missing metadata: {primary_column}");
    };
    current.merge(incoming)
  }
}

impl ParquetGeoColumnMetadata {
  pub(super) fn is_wkb(&self) -> bool {
    self.encoding == "WKB"
  }

  pub(super) fn parsed_geometry_types(&self) -> Result<Vec<ParsedGeometryType>> {
    self
      .geometry_types
      .iter()
      .map(|geometry_type| ParsedGeometryType::parse(geometry_type))
      .collect()
  }

  fn merge(&mut self, other: &Self) -> Result<()> {
    if self.encoding != other.encoding {
      bail!(
        "inconsistent geoparquet geometry encoding across input files: {} != {}",
        self.encoding,
        other.encoding
      );
    }
    if self.crs != other.crs {
      bail!("inconsistent geoparquet CRS across input files");
    }

    let mut geometry_types = self.geometry_types.iter().cloned().collect::<BTreeSet<_>>();
    geometry_types.extend(other.geometry_types.iter().cloned());
    self.geometry_types = geometry_types.into_iter().collect();
    self.bbox = merge_bbox(self.bbox.as_deref(), other.bbox.as_deref());
    Ok(())
  }
}

impl ParsedGeometryType {
  fn parse(value: &str) -> Result<Self> {
    let (base, has_z, has_m) = if let Some(base) = value.strip_suffix(" ZM") {
      (base, true, true)
    } else if let Some(base) = value.strip_suffix(" Z") {
      (base, true, false)
    } else if let Some(base) = value.strip_suffix(" M") {
      (base, false, true)
    } else {
      (value, false, false)
    };
    let kind = match base {
      "Point" => GeometryKind::Point,
      "LineString" => GeometryKind::LineString,
      "MultiPoint" => GeometryKind::MultiPoint,
      "MultiLineString" => GeometryKind::MultiLineString,
      "Polygon" => GeometryKind::Polygon,
      "MultiPolygon" => GeometryKind::MultiPolygon,
      "GeometryCollection" => GeometryKind::GeometryCollection,
      _ => bail!("unsupported geoparquet geometry type: {value}"),
    };
    Ok(Self { kind, has_z, has_m })
  }
}

impl ParquetInputSource {
  /// Parse and require consistent GeoParquet metadata across all discovered files.
  pub(super) fn geo_metadata(&self) -> Result<Option<ParquetGeoMetadata>> {
    let mut geo_meta: Option<ParquetGeoMetadata> = None;
    let mut saw_geo = false;
    let mut saw_missing_geo = false;
    for metadata in &self.metadata {
      match Self::parse_geo_metadata(metadata).context("parse geoparquet metadata")? {
        Some(file_geo_meta) => {
          saw_geo = true;
          if let Some(existing) = geo_meta.as_mut() {
            existing.merge(file_geo_meta)?;
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

  pub(super) fn covering_metadata(
    &self,
    geometry_column: &str,
  ) -> Result<Option<SourceCoveringMetadata>> {
    let mut covering = None;
    let mut saw_missing = false;
    for metadata in &self.metadata {
      let file_covering = Self::metadata_json(metadata, "geo")?
        .as_ref()
        .and_then(|json| Self::covering_column(json, geometry_column));
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

  /// Preserve non-reserved key-value metadata exactly once across a file set.
  pub(super) fn passthrough_metadata(&self) -> Vec<KeyValue> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for metadata in &self.metadata {
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

  fn parse_geo_metadata(metadata: &ArrowReaderMetadata) -> Result<Option<ParquetGeoMetadata>> {
    let Some(mut json) = Self::metadata_json(metadata, "geo")? else {
      return Ok(None);
    };
    Self::apply_default_crs(&mut json)?;
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

  /// Apply the GeoParquet OGC:CRS84 default when a geometry column omits `crs`.
  /// Preserve explicit `null` because it declares an unknown coordinate reference system.
  fn apply_default_crs(json: &mut Value) -> Result<()> {
    let Some(columns) = json.get_mut("columns").and_then(Value::as_object_mut) else {
      return Ok(());
    };
    if columns.values().all(|column| column.get("crs").is_some()) {
      return Ok(());
    }

    let crs = SpatialRef::from_definition("OGC:CRS84")
      .context("load GeoParquet default CRS OGC:CRS84")?
      .to_projjson()
      .context("export GeoParquet default CRS as PROJJSON")?;
    let crs: Value =
      serde_json::from_str(&crs).context("decode GeoParquet default CRS PROJJSON")?;
    for column in columns.values_mut() {
      if let Some(column) = column.as_object_mut()
        && !column.contains_key("crs")
      {
        column.insert("crs".to_string(), crs.clone());
      }
    }
    Ok(())
  }
}

impl SourceGeometryMetadata {
  /// Construct normalized source geometry metadata from a GeoParquet contract.
  pub(super) fn from_geoparquet(
    geo_meta: &ParquetGeoMetadata,
    covering: Option<SourceCoveringMetadata>,
  ) -> Result<Option<SourceGeometryMetadata>> {
    let Some(column_meta) = geo_meta.primary_geometry() else {
      return Ok(None);
    };
    if !column_meta.is_wkb() {
      return Ok(None);
    }

    let parsed_geometry_types = column_meta.parsed_geometry_types()?;
    let geometry_types = parsed_geometry_types
      .iter()
      .map(|geometry_type| geometry_type.kind)
      .collect();

    Ok(Some(SourceGeometryMetadata {
      column: geo_meta.primary_column.clone(),
      encoding: GeometryEncoding::Wkb,
      geometry_types,
      bbox: bbox_from_geoparquet(column_meta.bbox.as_deref()),
      covering,
      projjson: column_meta.crs.clone(),
      has_z: parsed_geometry_types
        .iter()
        .any(|geometry_type| geometry_type.has_z),
      has_m: parsed_geometry_types
        .iter()
        .any(|geometry_type| geometry_type.has_m),
    }))
  }
}

fn bbox_from_geoparquet(bbox: Option<&[Option<f64>]>) -> Option<Extent2D> {
  let bbox = bbox?;
  if bbox.len() < 4 {
    return None;
  }
  Some(Extent2D {
    xmin: bbox[0]?,
    ymin: bbox[1]?,
    xmax: bbox[bbox.len() - 2]?,
    ymax: bbox[bbox.len() - 1]?,
  })
}

fn merge_bbox(
  current: Option<&[Option<f64>]>,
  incoming: Option<&[Option<f64>]>,
) -> Option<Vec<Option<f64>>> {
  match (
    bbox_from_geoparquet(current),
    bbox_from_geoparquet(incoming),
  ) {
    (Some(current), Some(incoming)) => Some(vec![
      Some(current.xmin.min(incoming.xmin)),
      Some(current.ymin.min(incoming.ymin)),
      Some(current.xmax.max(incoming.xmax)),
      Some(current.ymax.max(incoming.ymax)),
    ]),
    (Some(current), None) => Some(vec![
      Some(current.xmin),
      Some(current.ymin),
      Some(current.xmax),
      Some(current.ymax),
    ]),
    (None, Some(incoming)) => Some(vec![
      Some(incoming.xmin),
      Some(incoming.ymin),
      Some(incoming.xmax),
      Some(incoming.ymax),
    ]),
    (None, None) => None,
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use crate::geometry::{Extent2D, GeometryKind};

  use super::{ParquetGeoMetadata, ParquetInputSource, ParsedGeometryType, bbox_from_geoparquet};

  fn metadata(value: serde_json::Value) -> ParquetGeoMetadata {
    serde_json::from_value(value).unwrap()
  }

  #[test]
  fn omitted_crs_uses_geoparquet_default() {
    let mut metadata = json!({
      "columns": {
        "geometry": {
          "encoding": "WKB"
        }
      }
    });

    ParquetInputSource::apply_default_crs(&mut metadata).unwrap();

    assert_eq!(
      metadata["columns"]["geometry"]["crs"]["id"]["authority"],
      "OGC"
    );
    assert_eq!(
      metadata["columns"]["geometry"]["crs"]["id"]["code"],
      "CRS84"
    );
  }

  #[test]
  fn explicit_null_crs_remains_undefined() {
    let mut metadata = json!({
      "columns": {
        "geometry": {
          "encoding": "WKB",
          "crs": null
        }
      }
    });

    ParquetInputSource::apply_default_crs(&mut metadata).unwrap();

    assert!(metadata["columns"]["geometry"]["crs"].is_null());
  }

  #[test]
  fn bbox_normalization_uses_outer_xy_values_for_dimensioned_bounds() {
    let bbox = [
      Some(-1.0),
      Some(-2.0),
      Some(10.0),
      Some(20.0),
      Some(3.0),
      Some(4.0),
    ];
    let extent = bbox_from_geoparquet(Some(&bbox)).unwrap();

    assert_eq!(extent.xmin, -1.0);
    assert_eq!(extent.ymin, -2.0);
    assert_eq!(extent.xmax, 3.0);
    assert_eq!(extent.ymax, 4.0);
  }

  #[test]
  fn bbox_normalization_rejects_incomplete_bounds() {
    assert!(bbox_from_geoparquet(Some(&[Some(-1.0), Some(-2.0), Some(3.0)])).is_none());
    assert!(bbox_from_geoparquet(Some(&[None, None, None, None])).is_none());
    assert!(bbox_from_geoparquet(None).is_none());
  }

  #[test]
  fn geoparquet_geometry_type_inference_preserves_z_and_m_flags() {
    for (geometry_type, expected_z, expected_m) in [
      ("Point Z", true, false),
      ("Point M", false, true),
      ("Point ZM", true, true),
    ] {
      let parsed = ParsedGeometryType::parse(geometry_type).unwrap();
      assert_eq!(parsed.kind, GeometryKind::Point);
      assert_eq!(parsed.has_z, expected_z);
      assert_eq!(parsed.has_m, expected_m);
    }
  }

  #[test]
  fn geoparquet_geometry_type_inference_supports_all_geometry_kinds() {
    for (value, expected) in [
      ("Point", GeometryKind::Point),
      ("LineString", GeometryKind::LineString),
      ("MultiPoint", GeometryKind::MultiPoint),
      ("MultiLineString", GeometryKind::MultiLineString),
      ("Polygon", GeometryKind::Polygon),
      ("MultiPolygon", GeometryKind::MultiPolygon),
      ("GeometryCollection", GeometryKind::GeometryCollection),
    ] {
      assert_eq!(ParsedGeometryType::parse(value).unwrap().kind, expected);
    }
  }

  #[test]
  fn metadata_deserialization_tolerates_null_bbox_and_unknown_fields() {
    let parsed = metadata(json!({
      "version": "1.1.0",
      "primary_column": "geometry",
      "unknown": true,
      "columns": {
        "geometry": {
          "encoding": "WKB",
          "geometry_types": ["Point"],
          "bbox": [null, null, null, null],
          "crs": null,
          "orientation": "counterclockwise"
        }
      }
    }));

    assert!(parsed.primary_geometry().unwrap().is_wkb());
    assert!(bbox_from_geoparquet(parsed.primary_geometry().unwrap().bbox.as_deref()).is_none());
  }

  #[test]
  fn metadata_merge_unions_geometry_types_and_bbox() {
    let mut current = metadata(json!({
      "primary_column": "geometry",
      "columns": {
        "geometry": {
          "encoding": "WKB",
          "geometry_types": ["Point Z"],
          "bbox": [-10.0, -5.0, 1.0, 2.0],
          "crs": null
        }
      }
    }));
    let incoming = metadata(json!({
      "primary_column": "geometry",
      "columns": {
        "geometry": {
          "encoding": "WKB",
          "geometry_types": ["MultiPoint Z"],
          "bbox": [-20.0, 0.0, 30.0, 40.0],
          "crs": null
        }
      }
    }));

    current.merge(incoming).unwrap();

    let geometry = current.primary_geometry().unwrap();
    assert_eq!(
      geometry.geometry_types,
      vec!["MultiPoint Z".to_string(), "Point Z".to_string()]
    );
    assert_eq!(
      bbox_from_geoparquet(geometry.bbox.as_deref()),
      Some(Extent2D {
        xmin: -20.0,
        ymin: -5.0,
        xmax: 30.0,
        ymax: 40.0,
      })
    );
  }

  #[test]
  fn metadata_merge_rejects_incompatible_source_contracts() {
    let base = || {
      metadata(json!({
        "primary_column": "geometry",
        "columns": {
          "geometry": {
            "encoding": "WKB",
            "geometry_types": ["Point"],
            "crs": {"id": {"authority": "EPSG", "code": 4326}}
          }
        }
      }))
    };

    let mut primary = base();
    let other_primary = metadata(json!({
      "primary_column": "shape",
      "columns": {
        "shape": {
          "encoding": "WKB",
          "geometry_types": ["Point"],
          "crs": {"id": {"authority": "EPSG", "code": 4326}}
        }
      }
    }));
    assert!(primary.merge(other_primary).is_err());

    let mut encoding = base();
    let other_encoding = metadata(json!({
      "primary_column": "geometry",
      "columns": {
        "geometry": {
          "encoding": "point",
          "geometry_types": ["Point"],
          "crs": {"id": {"authority": "EPSG", "code": 4326}}
        }
      }
    }));
    assert!(encoding.merge(other_encoding).is_err());

    let mut crs = base();
    let other_crs = metadata(json!({
      "primary_column": "geometry",
      "columns": {
        "geometry": {
          "encoding": "WKB",
          "geometry_types": ["Point"],
          "crs": {"id": {"authority": "EPSG", "code": 3857}}
        }
      }
    }));
    assert!(crs.merge(other_crs).is_err());
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

    assert!(ParquetInputSource::covering_column(&mixed, "geometry").is_none());
    assert!(ParquetInputSource::covering_column(&deep, "geometry").is_none());
  }
}

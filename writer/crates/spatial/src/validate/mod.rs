//! Validates one optimized Parquet file or recursive partitioned dataset.

mod geometry;
mod metadata;
mod multifile;
mod report;
mod structure;
mod xz;
mod z;

use std::path::Path;

use anyhow::{Context, Result};

use crate::parquet_dataset::{DiscoveryMode, discover_parquet_dataset};

pub use report::{
  ValidationFailure, ValidationFinding, ValidationLocation, ValidationReport, ValidationRule,
  ValidationSeverity,
};

/// Validate one Parquet file or recursive partitioned directory as one SOP dataset.
pub fn validate(path: impl AsRef<Path>) -> Result<ValidationReport> {
  let path = path.as_ref();
  let dataset_path = path.to_path_buf();
  let files = discover_parquet_dataset(path, DiscoveryMode::Recursive)?.with_context(|| {
    format!(
      "validation path must be a .parquet file or directory: {}",
      path.display()
    )
  })?;
  let mut report = ValidationReport::new(dataset_path);
  validate_dataset(&files, &mut report);
  report.sort_findings();
  Ok(report)
}

fn validate_dataset(
  files: &[crate::parquet_dataset::ParquetDatasetFile],
  report: &mut ValidationReport,
) {
  let validated_files = structure::load_dataset_files(files, report);
  let contracts = metadata::validate_dataset_metadata(&validated_files, report);
  structure::validate_dataset_structure(&validated_files, &contracts, report);
  let ranges = structure::validate_file_data(&validated_files, &contracts, report);
  multifile::validate_multifile_ranges(&ranges, report);
}

#[cfg(test)]
mod tests {
  use std::fs::File;
  use std::path::Path;
  use std::sync::Arc;

  use arrow_array::{BinaryArray, Float64Array, RecordBatch, UInt64Array};
  use arrow_schema::{DataType, Field, Schema, SchemaRef};
  use gdal::spatial_ref::SpatialRef;
  use parquet::arrow::arrow_writer::ArrowWriter;
  use parquet::basic::Compression;
  use parquet::file::metadata::KeyValue;
  use parquet::file::properties::WriterProperties;
  use tempfile::TempDir;

  use crate::geometry::Extent2D;
  use crate::optimized::point_z_code;

  use super::*;

  fn wkb_point(x: f64, y: f64) -> Vec<u8> {
    let geometry = geo::Geometry::Point(geo::Point::new(x, y));
    crate::geometry::write_test_geometry(&geometry)
  }

  fn write_parquet(
    path: &Path,
    schema: &SchemaRef,
    batches: &[RecordBatch],
    compression: Compression,
    metadata: &[KeyValue],
  ) {
    let properties = WriterProperties::builder()
      .set_compression(compression)
      .build();
    let mut writer = ArrowWriter::try_new(
      File::create(path).unwrap(),
      schema.clone(),
      Some(properties),
    )
    .unwrap();
    for batch in batches {
      writer.write(batch).unwrap();
    }
    for entry in metadata {
      writer.append_key_value_metadata(entry.clone());
    }
    writer.close().unwrap();
  }

  #[test]
  fn metadata_accepts_matching_wgs84_and_web_mercator() {
    for epsg in [4326, 3857] {
      let temp = TempDir::new().unwrap();
      let path = temp.path().join(format!("{epsg}.parquet"));
      write_z_fixture(&path, epsg, epsg, false, None);

      let report = validate(&path).unwrap();

      assert!(
        !report.has_errors(),
        "unexpected EPSG:{epsg} validation errors:\n{report}"
      );
    }
  }

  #[test]
  fn metadata_rejects_crs_mismatch_and_duplicate_reserved_entries() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("invalid.parquet");
    write_z_fixture(&path, 3857, 4326, true, None);

    let report = validate(&path).unwrap();

    assert!(
      report
        .findings()
        .iter()
        .any(|finding| finding.rule() == ValidationRule::Crs)
    );
    assert!(
      report
        .findings()
        .iter()
        .any(|finding| finding.rule() == ValidationRule::MetadataDuplicate)
    );
  }

  #[test]
  fn metadata_reports_each_missing_reserved_entry() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("missing.parquet");
    let schema = Arc::new(Schema::new(vec![Field::new(
      "geometry",
      DataType::Binary,
      true,
    )]));
    let geometry = wkb_point(1.0, 1.0);
    let batch = RecordBatch::try_new(
      schema.clone(),
      vec![Arc::new(BinaryArray::from(vec![Some(geometry.as_slice())]))],
    )
    .unwrap();
    write_parquet(
      &path,
      &schema,
      &[batch],
      parquet::basic::Compression::SNAPPY,
      &[],
    );

    let report = validate(&path).unwrap();
    let missing_count = report
      .findings()
      .iter()
      .filter(|finding| finding.rule() == ValidationRule::MetadataMissing)
      .count();

    assert_eq!(missing_count, 2);
  }

  #[test]
  fn metadata_rejects_unresolvable_wkt_even_with_supported_wkid() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("invalid-wkt.parquet");
    write_z_fixture(&path, 4326, 4326, false, Some("LOCAL_CS[\"unsupported\"]"));

    let report = validate(&path).unwrap();

    assert!(
      report
        .findings()
        .iter()
        .any(|finding| finding.rule() == ValidationRule::Crs
          && finding.location().column() == Some("geodisplay.wkt"))
    );
  }

  fn write_z_fixture(
    path: &std::path::Path,
    geo_epsg: u32,
    display_epsg: u32,
    duplicate_geo: bool,
    display_wkt: Option<&str>,
  ) {
    let extent = Extent2D {
      xmin: 0.0,
      ymin: 0.0,
      xmax: 10.0,
      ymax: 10.0,
    };
    let geometry = wkb_point(1.0, 1.0);
    let code = point_z_code(extent, 1.0, 1.0, 20).value();
    let schema = Arc::new(Schema::new(vec![
      Field::new("geometry", DataType::Binary, true),
      Field::new("zCode", DataType::UInt64, false),
      Field::new("x", DataType::Float64, false),
      Field::new("y", DataType::Float64, false),
    ]));
    let batch = RecordBatch::try_new(
      schema.clone(),
      vec![
        Arc::new(BinaryArray::from(vec![Some(geometry.as_slice())])),
        Arc::new(UInt64Array::from(vec![code])),
        Arc::new(Float64Array::from(vec![1.0])),
        Arc::new(Float64Array::from(vec![1.0])),
      ],
    )
    .unwrap();
    let crs = SpatialRef::from_epsg(geo_epsg)
      .unwrap()
      .to_projjson()
      .unwrap();
    let crs: serde_json::Value = serde_json::from_str(&crs).unwrap();
    let geo = serde_json::json!({
      "version": "1.1.0",
      "primary_column": "geometry",
      "columns": {
        "geometry": {
          "encoding": "WKB",
          "geometry_types": ["Point"],
          "bbox": [0.0, 0.0, 10.0, 10.0],
          "crs": crs
        }
      }
    });
    let mut geodisplay = serde_json::json!({
      "type": "z",
      "version": "0.1",
      "code": "zCode",
      "wkid": display_epsg,
      "xColumn": "x",
      "yColumn": "y",
      "coordinatePrecision": 20,
      "fullExtent": {
        "xmin": 0.0,
        "ymin": 0.0,
        "xmax": 10.0,
        "ymax": 10.0
      },
      "geometryType": "point",
      "hasZ": false,
      "hasM": false
    });
    if let Some(display_wkt) = display_wkt {
      geodisplay["wkt"] = serde_json::Value::String(display_wkt.to_string());
    }
    let geo_entry = KeyValue::new("geo".to_string(), Some(geo.to_string()));
    let mut entries = vec![
      geo_entry.clone(),
      KeyValue::new("geodisplay".to_string(), Some(geodisplay.to_string())),
    ];
    if duplicate_geo {
      entries.push(geo_entry);
    }
    write_parquet(
      path,
      &schema,
      &[batch],
      parquet::basic::Compression::SNAPPY,
      &entries,
    );
  }
}

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

use crate::parquet_dataset::{DiscoveryMode, try_discover_parquet_dataset};

pub use report::{
  ValidationFailure, ValidationFinding, ValidationLocation, ValidationReport, ValidationRule,
  ValidationSeverity,
};

/// Validate one Parquet file or recursive partitioned directory as one SOP dataset.
pub fn validate(path: impl AsRef<Path>) -> Result<ValidationReport> {
  let path = path.as_ref();
  let dataset_path = path.to_path_buf();
  let files = try_discover_parquet_dataset(path, DiscoveryMode::Recursive)?.with_context(|| {
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
  let validated_dataset_files = metadata::validate_dataset_metadata(&validated_files, report);
  structure::validate_dataset_structure(&validated_files, &validated_dataset_files, report);
  let ranges = structure::validate_file_data(&validated_dataset_files, report);
  multifile::validate_multifile_ranges(&ranges, report);
}

#[cfg(test)]
mod tests {
  use std::fs::File;
  use std::path::Path;
  use std::sync::Arc;

  use arrow_array::{ArrayRef, BinaryArray, Float64Array, RecordBatch, UInt64Array};
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
      write_z_fixture(&path, epsg, Some(epsg), false, None);

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
    write_z_fixture(&path, 3857, Some(4326), true, None);

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
  fn metadata_rejects_wkid_and_wkt_together() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("duplicate-crs.parquet");
    write_z_fixture(
      &path,
      4326,
      Some(4326),
      false,
      Some("GEOGCRS[\"WGS 84\",ID[\"EPSG\",4326]]"),
    );

    let report = validate(&path).unwrap();

    assert!(
      report
        .findings()
        .iter()
        .any(|finding| finding.rule() == ValidationRule::Crs
          && finding.message() == "geodisplay must define either wkid or wkt, not both")
    );
  }

  #[test]
  fn metadata_rejects_unresolvable_wkt() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("invalid-wkt.parquet");
    write_z_fixture(&path, 4326, None, false, Some("LOCAL_CS[\"unsupported\"]"));

    let report = validate(&path).unwrap();

    assert!(
      report
        .findings()
        .iter()
        .any(|finding| finding.rule() == ValidationRule::Crs
          && finding.location().column() == Some("geodisplay.wkt"))
    );
  }

  #[test]
  fn data_validation_reports_malformed_z_batch_views() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("wrong-z-code-type.parquet");
    write_z_fixture_with_code_type(&path, 4326, Some(4326), false, None, DataType::Float64);

    let report = validate(&path).unwrap();

    assert!(report.findings().iter().any(|finding| {
      finding.rule() == ValidationRule::RowGroup
        && finding.message() == "Arrow column 'zCode' is not UInt64"
    }));
  }

  fn write_z_fixture(
    path: &std::path::Path,
    geo_epsg: u32,
    display_epsg: Option<u32>,
    duplicate_geo: bool,
    display_wkt: Option<&str>,
  ) {
    write_z_fixture_with_code_type(
      path,
      geo_epsg,
      display_epsg,
      duplicate_geo,
      display_wkt,
      DataType::UInt64,
    );
  }

  fn write_z_fixture_with_code_type(
    path: &std::path::Path,
    geo_epsg: u32,
    display_epsg: Option<u32>,
    duplicate_geo: bool,
    display_wkt: Option<&str>,
    code_type: DataType,
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
      Field::new("zCode", code_type.clone(), false),
      Field::new("x", DataType::Float64, false),
      Field::new("y", DataType::Float64, false),
    ]));
    let code_values: ArrayRef = match code_type {
      DataType::UInt64 => Arc::new(UInt64Array::from(vec![code])),
      DataType::Float64 => Arc::new(Float64Array::from(vec![code as f64])),
      _ => unreachable!("test fixture only supports supported Z code types"),
    };
    let batch = RecordBatch::try_new(
      schema.clone(),
      vec![
        Arc::new(BinaryArray::from(vec![Some(geometry.as_slice())])),
        code_values,
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
      "parentColumn": null,
      "index": {
        "type": "z",
        "version": "0.1",
        "code": "zCode",
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
      }
    });
    if let Some(display_epsg) = display_epsg {
      geodisplay["index"]["wkid"] = serde_json::Value::from(display_epsg);
    }
    if let Some(display_wkt) = display_wkt {
      geodisplay["index"]["wkt"] = serde_json::Value::String(display_wkt.to_string());
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

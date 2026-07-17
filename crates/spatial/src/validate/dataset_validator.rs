//! Coordinates validation for one optimized Parquet file or recursive partitioned dataset.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound;
use std::path::Path;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::parquet_dataset::{
  DiscoveryMode, ParquetDatasetFile, PartitionDescriptor, PartitionFamily,
};

use super::file_validator::FileValidator;
use super::metadata_validator::MetadataValidator;
use super::report::{ValidationLocation, ValidationReport, ValidationRule, ValidationSeverity};

/// Coordinates all validation phases for one SOP dataset.
pub struct DatasetValidator;

impl DatasetValidator {
  /// Validate one Parquet file or recursive partitioned directory as one SOP dataset.
  pub fn validate(path: impl AsRef<Path>) -> Result<ValidationReport> {
    let path = path.as_ref();
    let dataset_path = path.to_path_buf();
    let files = DiscoveryMode::Recursive.discover(path)?.with_context(|| {
      format!(
        "validation path must be a .parquet file or directory: {}",
        path.display()
      )
    })?;
    let mut report = ValidationReport::new(dataset_path);
    Self::validate_dataset(&files, &mut report);
    report.sort_findings();
    Ok(report)
  }

  fn validate_dataset(files: &[ParquetDatasetFile], report: &mut ValidationReport) {
    let validated_files = FileValidator::load_all(files, report);
    let validated_dataset_files = MetadataValidator::validate_dataset(&validated_files, report);
    FileValidator::validate_dataset_structure(&validated_files, &validated_dataset_files, report);
    let ranges = FileValidator::validate_file_data(&validated_dataset_files, report);
    Self::validate_clustering_ranges(&ranges, report);
  }

  fn validate_clustering_ranges(ranges: &[ClusteringRange], report: &mut ValidationReport) {
    Self::validate_clustering_families(ranges, report);
    Self::validate_partition_bounds(ranges, report);
    Self::warn_overlapping_ranges(ranges, report);
  }

  fn validate_clustering_families(ranges: &[ClusteringRange], report: &mut ValidationReport) {
    let mut families = BTreeSet::new();
    for range in ranges {
      families.insert(range.family);
    }
    if families.len() > 1 {
      report.push(
        ValidationRule::DatasetConsistency,
        ValidationSeverity::Error,
        ValidationLocation::default(),
        "dataset mixes Z and XZ clustering families",
      );
    }
  }

  fn validate_partition_bounds(ranges: &[ClusteringRange], report: &mut ValidationReport) {
    let mut lower_bounds = BTreeMap::<PartitionFamily, BTreeSet<u64>>::new();
    for range in ranges {
      if let Some(partition) = range.partition
        && partition.family == range.family
      {
        lower_bounds
          .entry(range.family)
          .or_default()
          .insert(partition.lower_bound);
      }
    }

    for range in ranges {
      let Some(partition) = range.partition else {
        continue;
      };
      if partition.family != range.family {
        continue;
      }
      if range.minimum < partition.lower_bound {
        report.push(
          ValidationRule::Partition,
          ValidationSeverity::Error,
          ValidationLocation::file(range.file.clone()),
          format!(
            "observed clustering code {} falls below declared partition lower bound {}",
            range.minimum, partition.lower_bound
          ),
        );
      }
      let next_bound = lower_bounds.get(&range.family).and_then(|bounds| {
        bounds
          .range((Bound::Excluded(partition.lower_bound), Bound::Unbounded))
          .next()
          .copied()
      });
      if let Some(next_bound) = next_bound
        && range.maximum >= next_bound
      {
        report.push(
          ValidationRule::Partition,
          ValidationSeverity::Error,
          ValidationLocation::file(range.file.clone()),
          format!(
            "observed clustering code {} reaches the next partition lower bound {}",
            range.maximum, next_bound
          ),
        );
      }
    }
  }

  fn warn_overlapping_ranges(ranges: &[ClusteringRange], report: &mut ValidationReport) {
    for family in [PartitionFamily::Z, PartitionFamily::Xz] {
      let mut family_ranges = ranges
        .iter()
        .filter(|range| range.family == family)
        .collect::<Vec<_>>();
      family_ranges.sort_by_key(|range| (range.minimum, range.maximum, &range.file));
      let mut maximum = None::<(u64, &PathBuf)>;
      for range in family_ranges {
        if let Some((previous_maximum, previous_file)) = maximum
          && range.minimum <= previous_maximum
        {
          report.push(
            ValidationRule::RangeOverlap,
            ValidationSeverity::Warning,
            ValidationLocation::file(range.file.clone()),
            format!(
              "clustering range {}..={} overlaps {}",
              range.minimum,
              range.maximum,
              previous_file.display()
            ),
          );
        }
        if maximum.is_none_or(|(previous_maximum, _)| range.maximum > previous_maximum) {
          maximum = Some((range.maximum, &range.file));
        }
      }
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClusteringRange {
  pub(crate) file: PathBuf,
  pub(crate) family: PartitionFamily,
  pub(crate) minimum: u64,
  pub(crate) maximum: u64,
  pub(crate) partition: Option<PartitionDescriptor>,
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
  use crate::optimized::ClusterKey;

  use super::*;

  #[test]
  fn warns_for_overlapping_clustering_ranges() {
    let ranges = vec![
      ClusteringRange {
        file: PathBuf::from("a.parquet"),
        family: PartitionFamily::Z,
        minimum: 0,
        maximum: 10,
        partition: None,
      },
      ClusteringRange {
        file: PathBuf::from("b.parquet"),
        family: PartitionFamily::Z,
        minimum: 10,
        maximum: 20,
        partition: None,
      },
    ];
    let mut report = ValidationReport::new(PathBuf::from("dataset"));

    DatasetValidator::validate_clustering_ranges(&ranges, &mut report);

    assert_eq!(report.warning_count(), 1);
    assert_eq!(report.findings()[0].rule(), ValidationRule::RangeOverlap);
  }

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

      let report = DatasetValidator::validate(&path).unwrap();

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

    let report = DatasetValidator::validate(&path).unwrap();

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

    let report = DatasetValidator::validate(&path).unwrap();

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

    let report = DatasetValidator::validate(&path).unwrap();

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

    let report = DatasetValidator::validate(&path).unwrap();

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
    let code = ClusterKey::from_z_coordinates(extent, 1.0, 1.0, 20).value();
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

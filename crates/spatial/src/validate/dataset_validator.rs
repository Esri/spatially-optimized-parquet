// Copyright 2026 Esri
//
// Licensed under the Apache License Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Coordinates validation for one optimized Parquet file or recursive partitioned dataset.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound;
use std::path::Path;
use std::path::PathBuf;

use crate::input::parquet::{DiscoveryMode, ParquetDataset, PartitionDescriptor, PartitionFamily};

use super::ValidationError;
use super::file_validator::FileValidator;
use super::metadata_validator::MetadataValidator;
use super::report::{ValidationLocation, ValidationReport, ValidationRule, ValidationSeverity};

/// Coordinates all validation phases for one SOP dataset.
pub struct DatasetValidator;

impl DatasetValidator {
  /// Validate one Parquet file or recursive partitioned directory as one SOP dataset.
  pub fn validate(path: impl AsRef<Path>) -> Result<ValidationReport, ValidationError> {
    let path = path.as_ref();
    let dataset = ParquetDataset::discover(path, DiscoveryMode::Recursive)?.ok_or_else(|| {
      ValidationError::InvalidPath {
        path: path.to_path_buf(),
      }
    })?;
    let mut report = ValidationReport::new(dataset.root().to_path_buf());
    Self::validate_dataset(&dataset, &mut report);
    report.sort_findings();
    Ok(report)
  }

  /// Validate file metadata, schema, row data, and dataset clustering boundaries in order.
  fn validate_dataset(dataset: &ParquetDataset, report: &mut ValidationReport) {
    let validated_files = FileValidator::load_all(dataset.files(), report);
    let validated_dataset_files = MetadataValidator::validate_dataset(&validated_files, report);
    FileValidator::validate_dataset_structure(&validated_files, &validated_dataset_files, report);
    let ranges = FileValidator::validate_file_data(&validated_dataset_files, report);
    Self::validate_clustering_ranges(&ranges, report);
  }

  /// Validate clustering families and partition bounds before reporting range overlap.
  fn validate_clustering_ranges(ranges: &[ClusteringRange], report: &mut ValidationReport) {
    Self::validate_clustering_families(ranges, report);
    Self::validate_partition_bounds(ranges, report);
    Self::warn_overlapping_ranges(ranges, report);
  }

  /// Reject datasets that mix point Z and non-point XZ clustering.
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

  /// Validate observed codes against each partition's inclusive lower and exclusive upper bounds.
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

  /// Report overlapping file ranges independently within each clustering family.
  fn warn_overlapping_ranges(ranges: &[ClusteringRange], report: &mut ValidationReport) {
    for family in [PartitionFamily::Z, PartitionFamily::Xz] {
      let mut family_ranges = Vec::new();
      for range in ranges {
        if range.family == family {
          family_ranges.push(range);
        }
      }
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
  use std::sync::Arc;

  use arrow_array::{
    Array, ArrayRef, BinaryArray, Float64Array, RecordBatch, StructArray, UInt64Array,
  };
  use arrow_schema::{DataType, Field, Fields, Schema};
  use gdal::spatial_ref::SpatialRef;
  use parquet::arrow::arrow_writer::ArrowWriter;
  use parquet::basic::Compression;
  use parquet::file::metadata::KeyValue;
  use parquet::file::properties::WriterProperties;
  use tempfile::TempDir;

  use crate::geometry::{Coord, Extent2D};
  use crate::optimized::ClusterKey;

  use super::*;

  /// Own mutable wire metadata alongside a physically valid test batch.
  struct ValidationFixture {
    batch: RecordBatch,
    geo: serde_json::Value,
    display: serde_json::Value,
    extra_metadata: Vec<KeyValue>,
  }

  impl ValidationFixture {
    /// Create nullable point columns and codes matching the supplied WKB values.
    fn points(geometries: Vec<Option<Vec<u8>>>) -> Self {
      let coordinates: Vec<Option<Coord>> = geometries
        .iter()
        .map(|geometry| -> Option<Coord> {
          geometry
            .as_deref()
            .map(|bytes| -> Coord { Coord::from_point_wkb(bytes).unwrap() })
        })
        .collect();
      let extent = Extent2D {
        xmin: 0.0,
        ymin: 0.0,
        xmax: 10.0,
        ymax: 10.0,
      };
      let has_z = coordinates
        .iter()
        .flatten()
        .any(|coordinate| -> bool { coordinate.z.is_some() });
      let has_m = coordinates
        .iter()
        .flatten()
        .any(|coordinate| -> bool { coordinate.m.is_some() });
      let codes: Vec<u64> = coordinates
        .iter()
        .map(|coordinate| -> u64 {
          match coordinate {
            Some(coordinate) if coordinate.x.is_finite() && coordinate.y.is_finite() => {
              ClusterKey::from_z_coordinates(extent, coordinate.x, coordinate.y, 20).value()
            }
            Some(_) | None => 0,
          }
        })
        .collect();
      let schema = Arc::new(Schema::new(vec![
        Field::new("geometry", DataType::Binary, true),
        Field::new("zCode", DataType::UInt64, false),
        Field::new("x", DataType::Float64, true),
        Field::new("y", DataType::Float64, true),
        Field::new("z", DataType::Float64, true),
        Field::new("m", DataType::Float64, true),
      ]));
      let batch = RecordBatch::try_new(
        Arc::clone(&schema),
        vec![
          Arc::new(BinaryArray::from_iter(
            geometries
              .iter()
              .map(|value| -> Option<&[u8]> { value.as_deref() }),
          )),
          Arc::new(UInt64Array::from(codes)),
          Arc::new(Float64Array::from_iter(coordinates.iter().map(
            |value| -> Option<f64> { value.as_ref().map(|coordinate| coordinate.x) },
          ))),
          Arc::new(Float64Array::from_iter(coordinates.iter().map(
            |value| -> Option<f64> { value.as_ref().map(|coordinate| coordinate.y) },
          ))),
          Arc::new(Float64Array::from_iter(coordinates.iter().map(
            |value| -> Option<f64> { value.as_ref().and_then(|coordinate| coordinate.z) },
          ))),
          Arc::new(Float64Array::from_iter(coordinates.iter().map(
            |value| -> Option<f64> { value.as_ref().and_then(|coordinate| coordinate.m) },
          ))),
        ],
      )
      .unwrap();
      let crs: serde_json::Value =
        serde_json::from_str(&SpatialRef::from_epsg(4326).unwrap().to_projjson().unwrap()).unwrap();
      let geometry_type = match (has_z, has_m) {
        (false, false) => "Point",
        (true, false) => "Point Z",
        (false, true) => "Point M",
        (true, true) => "Point ZM",
      };
      let geo = serde_json::json!({
        "version": "2.0.0", "primary_column": "geometry",
        "columns": { "geometry": {
          "encoding": "WKB", "geometry_types": [geometry_type],
          "bbox": [0.0, 0.0, 10.0, 10.0], "crs": crs,
        }},
      });
      let mut display = serde_json::json!({
        "type": "z", "version": "0.1", "wkid": 4326, "geometryType": "point",
        "writer": { "name": "validator-test", "version": "1" },
        "code": "zCode", "xColumn": "x", "yColumn": "y", "coordinatePrecision": 20,
        "fullExtent": { "xmin": 0.0, "ymin": 0.0, "xmax": 10.0, "ymax": 10.0 },
        "hasZ": has_z, "hasM": has_m,
      });
      if has_z {
        display["zColumn"] = serde_json::json!("z");
      }
      if has_m {
        display["mColumn"] = serde_json::json!("m");
      }
      Self {
        batch,
        geo,
        display,
        extra_metadata: Vec::new(),
      }
    }

    /// Validate a real Parquet file and require success or the specified diagnostic rules.
    fn check(&self, expected: &[ValidationRule]) -> ValidationReport {
      let directory = TempDir::new().unwrap();
      let path = directory.path().join("fixture.parquet");
      let properties = WriterProperties::builder()
        .set_compression(Compression::SNAPPY)
        .build();
      let mut writer = ArrowWriter::try_new(
        File::create(&path).unwrap(),
        self.batch.schema(),
        Some(properties),
      )
      .unwrap();
      writer.write(&self.batch).unwrap();
      for (key, value) in [("geo", &self.geo), ("geodisplay", &self.display)] {
        writer.append_key_value_metadata(KeyValue::new(key.into(), Some(value.to_string())));
      }
      for entry in &self.extra_metadata {
        writer.append_key_value_metadata(entry.clone());
      }
      writer.close().unwrap();
      let report = DatasetValidator::validate(&path).unwrap();
      if expected.is_empty() {
        assert!(!report.has_errors(), "{report}");
      }
      for rule in expected {
        assert!(
          report
            .findings()
            .iter()
            .any(|finding| -> bool { finding.rule() == *rule }),
          "{rule:?}: {report}"
        );
      }
      report
    }

    /// Remove observed and normalization bounds without changing geometry declarations.
    fn omit_extents(&mut self) {
      self.geo["columns"]["geometry"]
        .as_object_mut()
        .unwrap()
        .remove("bbox");
      self.display.as_object_mut().unwrap().remove("fullExtent");
    }

    /// Replace one root column and its schema field while preserving all other columns.
    fn set_column(&mut self, name: &str, values: ArrayRef) {
      let schema = self.batch.schema();
      let column_index = schema.index_of(name).unwrap();
      let mut fields = schema.fields().to_vec();
      fields[column_index] = Arc::new(Field::new(name, values.data_type().clone(), true));
      let schema = Arc::new(Schema::new(fields));
      let mut arrays = self.batch.columns().to_vec();
      arrays[column_index] = values;
      self.batch = RecordBatch::try_new(schema, arrays).unwrap();
    }

    /// Nest selected display fields under a parent whose final row is null.
    fn nest_last_row(&mut self, columns: &[(&str, &str)]) {
      let schema = self.batch.schema();
      let indices: Vec<usize> = columns
        .iter()
        .map(|(name, _)| -> usize { schema.index_of(name).unwrap() })
        .collect();
      let fields = Fields::from(
        indices
          .iter()
          .map(|index| -> Arc<Field> {
            Arc::new(
              schema.fields()[*index]
                .as_ref()
                .clone()
                .with_nullable(false),
            )
          })
          .collect::<Vec<_>>(),
      );
      let children = indices
        .iter()
        .map(|index| -> ArrayRef { Arc::clone(self.batch.column(*index)) })
        .collect();
      let mut validity = vec![true; self.batch.num_rows()];
      *validity.last_mut().unwrap() = false;
      let parent = StructArray::new(fields, children, Some(validity.into()));
      let mut fields = Vec::new();
      let mut arrays = Vec::new();
      for (index, field) in schema.fields().iter().enumerate() {
        if !indices.contains(&index) {
          fields.push(Arc::clone(field));
          arrays.push(Arc::clone(self.batch.column(index)));
        }
      }
      fields.push(Arc::new(Field::new(
        "holder",
        parent.data_type().clone(),
        true,
      )));
      arrays.push(Arc::new(parent));
      self.batch = RecordBatch::try_new(Arc::new(Schema::new(fields)), arrays).unwrap();
      for (name, metadata_name) in columns {
        self.display[*metadata_name] = serde_json::json!(["holder", name]);
      }
    }
  }

  /// Reject explicit null bounds while retaining optional omission.
  #[test]
  fn optional_extents_reject_json_null() {
    let mut fixture =
      ValidationFixture::points(vec![Some(crate::geometry::write_test_point(1.0, 1.0))]);
    fixture.geo["columns"]["geometry"]
      .as_object_mut()
      .unwrap()
      .remove("bbox");
    fixture.check(&[]);

    for (field, entry) in [("bbox", "geo"), ("fullExtent", "geodisplay")] {
      fixture.omit_extents();
      if field == "bbox" {
        fixture.geo["columns"]["geometry"][field] = serde_json::Value::Null;
      } else {
        fixture.display[field] = serde_json::Value::Null;
      }
      let report = fixture.check(&[ValidationRule::MetadataMalformed]);
      assert!(report.findings().iter().any(|finding| -> bool {
        finding.rule() == ValidationRule::MetadataMalformed
          && finding.location().column() == Some(entry)
      }));
    }
  }

  /// Require an extent for any finite point, including one beyond the usual geometry sample.
  #[test]
  fn normalization_extent_depends_on_source_locations() {
    let indeterminate = Some(crate::geometry::write_test_point(f64::NAN, f64::NAN));
    let mut finite_tail = vec![indeterminate.clone(); 4];
    finite_tail.push(Some(crate::geometry::write_test_point(1.0, 1.0)));
    for (geometries, expected) in [
      (Vec::new(), Vec::new()),
      (vec![None; 5], Vec::new()),
      (vec![indeterminate; 5], Vec::new()),
      (finite_tail, vec![ValidationRule::Extent]),
    ] {
      let mut fixture = ValidationFixture::points(geometries);
      fixture.set_column(
        "zCode",
        Arc::new(UInt64Array::from(vec![None; fixture.batch.num_rows()])),
      );
      fixture.omit_extents();
      let report = fixture.check(&expected);
      if !expected.is_empty() {
        assert!(report.findings().iter().any(|finding| -> bool {
          finding.rule() == ValidationRule::Extent
            && finding.location().column() == Some("geodisplay.fullExtent")
        }));
      }
    }
  }

  /// Honor parent validity for every point coordinate and the clustering key after the sample.
  #[test]
  fn nested_codes_and_coordinates_honor_parent_validity() {
    let mut point = vec![1];
    point.extend_from_slice(&3001_u32.to_le_bytes());
    for coordinate in [1.0_f64, 1.0, 0.0, 0.0] {
      point.extend_from_slice(&coordinate.to_le_bytes());
    }
    for (column, metadata_name, rule) in [
      ("zCode", "code", ValidationRule::ZCode),
      ("x", "xColumn", ValidationRule::ZCoordinate),
      ("y", "yColumn", ValidationRule::ZCoordinate),
      ("z", "zColumn", ValidationRule::ZCoordinate),
      ("m", "mColumn", ValidationRule::ZCoordinate),
    ] {
      let mut fixture = ValidationFixture::points(vec![Some(point.clone()); 5]);
      fixture.nest_last_row(&[(column, metadata_name)]);
      fixture.check(&[rule]);
    }
  }

  /// Ignore null geometry's coordinate payload and allow an absent nested clustering code.
  #[test]
  fn null_geometry_does_not_require_coordinate_sentinels() {
    let mut fixture = ValidationFixture::points(vec![None; 5]);
    fixture.omit_extents();
    fixture.set_column(
      "x",
      Arc::new(Float64Array::from(vec![
        None,
        Some(f64::NAN),
        Some(0.0),
        Some(5.0),
        None,
      ])),
    );
    fixture.set_column(
      "y",
      Arc::new(Float64Array::from(vec![
        Some(0.0),
        None,
        Some(f64::NAN),
        Some(5.0),
        None,
      ])),
    );
    fixture.set_column("zCode", Arc::new(UInt64Array::from(vec![Some(5); 5])));
    fixture.check(&[]);
    fixture.nest_last_row(&[("zCode", "code")]);
    fixture.check(&[]);
  }

  /// Apply the extent exception and effective code validity to XZ input as well as Z input.
  #[test]
  fn xz_checks_optional_extent_and_nested_codes() {
    let mut fixture = ValidationFixture::points(vec![None; 5]);
    fixture.geo["columns"]["geometry"]["geometry_types"] = serde_json::json!(["MultiPoint"]);
    fixture.omit_extents();
    fixture.display = serde_json::json!({
      "type": "xz", "version": "0.1", "wkid": 4326, "geometryType": "multipoint",
      "writer": { "name": "validator-test", "version": "1" },
      "code": "zCode", "maxLevel": 20, "encoding": "esriPBF",
      "hasZ": false, "hasM": false, "levels": [],
    });
    fixture.check(&[]);

    let empty = vec![1, 4, 0, 0, 0, 0, 0, 0, 0];
    fixture.set_column("zCode", Arc::new(UInt64Array::from(vec![None; 5])));
    let mut geometries = vec![empty; 5];
    fixture.set_column(
      "geometry",
      Arc::new(BinaryArray::from_iter_values(
        geometries.iter().map(Vec::as_slice),
      )),
    );
    fixture.check(&[]);

    let mut finite = vec![1, 4, 0, 0, 0, 1, 0, 0, 0];
    finite.extend(crate::geometry::write_test_point(1.0, 1.0));
    geometries[4] = finite;
    fixture.set_column(
      "geometry",
      Arc::new(BinaryArray::from_iter_values(
        geometries.iter().map(Vec::as_slice),
      )),
    );
    fixture.check(&[ValidationRule::Extent]);

    fixture.display["fullExtent"] =
      serde_json::json!({ "xmin": 0.0, "ymin": 0.0, "xmax": 10.0, "ymax": 10.0 });
    fixture.set_column("zCode", Arc::new(UInt64Array::from(vec![Some(0); 5])));
    fixture.nest_last_row(&[("zCode", "code")]);
    fixture.check(&[ValidationRule::XzCode]);
  }

  /// Do not confuse an unknown display component with a finite source ordinate.
  #[test]
  fn nullable_components_must_preserve_finite_source_values() {
    for (source_z, stored_z, expected) in [
      (0.0, Some(f64::INFINITY), vec![ValidationRule::ZCoordinate]),
      (0.0, None, vec![ValidationRule::ZCoordinate]),
      (f64::NAN, None, vec![]),
      (f64::NAN, Some(f64::NAN), vec![]),
    ] {
      let mut point = vec![1];
      point.extend_from_slice(&1001_u32.to_le_bytes());
      for coordinate in [1.0_f64, 1.0, source_z] {
        point.extend_from_slice(&coordinate.to_le_bytes());
      }
      let mut fixture = ValidationFixture::points(vec![Some(point); 5]);
      let mut components = vec![Some(source_z); 5];
      components[4] = stored_z;
      fixture.set_column("z", Arc::new(Float64Array::from(components)));
      fixture.check(&expected);
    }
  }

  /// Ignore null codes for ordering without allowing them to hide determinate source rows.
  #[test]
  fn nullable_codes_do_not_hide_finite_points() {
    let mut geometries = vec![
      None,
      Some(crate::geometry::write_test_point(1.0, 1.0)),
      None,
    ];
    for coordinate in [2.0, 3.0, 4.0, 5.0] {
      geometries.push(Some(crate::geometry::write_test_point(
        coordinate, coordinate,
      )));
    }
    let mut fixture = ValidationFixture::points(geometries);
    let mut codes: Vec<Option<u64>> = fixture
      .batch
      .column(1)
      .as_any()
      .downcast_ref::<UInt64Array>()
      .unwrap()
      .iter()
      .collect();
    codes[0] = None;
    codes[2] = None;
    fixture.set_column("zCode", Arc::new(UInt64Array::from(codes.clone())));
    fixture.check(&[]);

    *codes.last_mut().unwrap() = None;
    fixture.set_column("zCode", Arc::new(UInt64Array::from(codes)));
    fixture.check(&[ValidationRule::ZCode]);
  }

  /// Accept a completely null display group only when its source geometry is also indeterminate.
  #[test]
  fn a_whole_display_group_can_be_null() {
    let mut geometries = vec![Some(crate::geometry::write_test_point(1.0, 1.0)); 4];
    geometries.push(None);
    let mut fixture = ValidationFixture::points(geometries.clone());
    fixture.nest_last_row(&[("zCode", "code"), ("x", "xColumn"), ("y", "yColumn")]);
    fixture.check(&[]);

    geometries[4] = Some(crate::geometry::write_test_point(1.0, 1.0));
    fixture.set_column(
      "geometry",
      Arc::new(BinaryArray::from_iter(
        geometries
          .iter()
          .map(|value| -> Option<&[u8]> { value.as_deref() }),
      )),
    );
    fixture.check(&[ValidationRule::ZCode]);
  }

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

  /// Accept both supported CRS declarations through the shared fixture.
  #[test]
  fn metadata_accepts_matching_wgs84_and_web_mercator() {
    for epsg in [4326, 3857] {
      let mut fixture =
        ValidationFixture::points(vec![Some(crate::geometry::write_test_point(1.0, 1.0))]);
      fixture.geo["columns"]["geometry"]["crs"] =
        serde_json::from_str(&SpatialRef::from_epsg(epsg).unwrap().to_projjson().unwrap()).unwrap();
      fixture.geo["version"] = serde_json::json!("1.1.0");
      fixture.display["wkid"] = serde_json::json!(epsg);
      fixture.check(&[]);
    }
  }

  /// Report both CRS mismatch and duplicate metadata rather than hiding one behind the other.
  #[test]
  fn metadata_rejects_crs_mismatch_and_duplicate_reserved_entries() {
    let mut fixture =
      ValidationFixture::points(vec![Some(crate::geometry::write_test_point(1.0, 1.0))]);
    fixture.display["wkid"] = serde_json::json!(3857);
    fixture
      .extra_metadata
      .push(KeyValue::new("geo".into(), Some(fixture.geo.to_string())));
    fixture.check(&[ValidationRule::Crs, ValidationRule::MetadataDuplicate]);
  }

  /// Reject conflicting CRS declarations and WKT outside the supported references.
  #[test]
  fn metadata_rejects_invalid_wkt_configuration() {
    for (wkid, wkt, expected_column) in [
      (
        Some(4326),
        "GEOGCRS[\"WGS 84\",ID[\"EPSG\",4326]]",
        "geodisplay",
      ),
      (None, "LOCAL_CS[\"unsupported\"]", "geodisplay.wkt"),
    ] {
      let mut fixture =
        ValidationFixture::points(vec![Some(crate::geometry::write_test_point(1.0, 1.0))]);
      fixture.display["wkid"] = serde_json::json!(wkid);
      fixture.display["wkt"] = serde_json::json!(wkt);
      let report = fixture.check(&[ValidationRule::Crs]);
      assert!(report.findings().iter().any(|finding| -> bool {
        finding.rule() == ValidationRule::Crs
          && finding.location().column() == Some(expected_column)
      }));
      if wkid.is_some() {
        assert!(report.findings().iter().any(|finding| -> bool {
          finding.message() == "geodisplay must define either wkid or wkt, not both"
        }));
      }
    }
  }

  /// Report an operational column-view failure as well as schema incompatibility.
  #[test]
  fn data_validation_reports_malformed_z_batch_views() {
    let mut fixture =
      ValidationFixture::points(vec![Some(crate::geometry::write_test_point(1.0, 1.0))]);
    fixture.set_column("zCode", Arc::new(Float64Array::from(vec![0.0])));
    let report = fixture.check(&[ValidationRule::RowGroup]);
    assert!(report.findings().iter().any(|finding| -> bool {
      finding.rule() == ValidationRule::RowGroup
        && finding.message() == "Arrow column 'zCode' is not UInt64"
    }));
  }

  /// Reject unknown discriminants at the metadata boundary.
  #[test]
  fn metadata_rejects_unknown_geodisplay_vocabulary() {
    for (field, value) in [("type", "future"), ("geometryType", "futureGeometry")] {
      let mut fixture =
        ValidationFixture::points(vec![Some(crate::geometry::write_test_point(1.0, 1.0))]);
      fixture.display[field] = serde_json::json!(value);
      let report = fixture.check(&[ValidationRule::MetadataMalformed]);
      assert!(report.findings().iter().any(|finding| -> bool {
        finding.rule() == ValidationRule::MetadataMalformed
          && finding.location().column() == Some("geodisplay")
      }));
    }
  }
}

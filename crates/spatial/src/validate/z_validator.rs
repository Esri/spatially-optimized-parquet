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

use arrow_array::Array;
use arrow_schema::DataType;

use crate::geometry::Coord;
use crate::input::parquet::PartitionFamily;
use crate::optimized::ClusterKey;
use crate::optimized::ClusteringIndexZ;

use super::dataset_validator::ClusteringRange;
use super::file_validator::FileValidator;
use super::geometry_validator::GeometryValidator;
use super::metadata_validator::{MetadataValidator, ValidatedMetadata};
use super::report::{ValidationLocation, ValidationReport, ValidationRule, ValidationSeverity};

pub(crate) struct ZValidator;

impl ZValidator {
  /// Validate point clustering codes and required XY and optional Z/M Float64 columns.
  pub(crate) fn validate_schema(
    index: &ClusteringIndexZ,
    file: &FileValidator,
    report: &mut ValidationReport,
  ) {
    Self::validate_field(file, &index.code.dotted(), &DataType::UInt64, report);
    Self::validate_field(file, &index.x_column.dotted(), &DataType::Float64, report);
    Self::validate_field(file, &index.y_column.dotted(), &DataType::Float64, report);
    Self::validate_dimension_field(
      file,
      index
        .z_column
        .as_ref()
        .map(crate::optimized::ColumnPath::dotted),
      index.has_z,
      "zColumn",
      report,
    );
    Self::validate_dimension_field(
      file,
      index
        .m_column
        .as_ref()
        .map(crate::optimized::ColumnPath::dotted),
      index.has_m,
      "mColumn",
      report,
    );
  }

  /// Validate Z ordering and null rows, sample four non-null geometries, and collect code bounds.
  pub(crate) fn validate_file(
    index: &ClusteringIndexZ,
    file: &FileValidator,
    contract: &ValidatedMetadata,
    report: &mut ValidationReport,
  ) -> Option<ClusteringRange> {
    let code_path = index.code.dotted();
    let x_path = index.x_column.dotted();
    let y_path = index.y_column.dotted();
    let z_path = index
      .z_column
      .as_ref()
      .map(crate::optimized::ColumnPath::dotted);
    let m_path = index
      .m_column
      .as_ref()
      .map(crate::optimized::ColumnPath::dotted);
    let mut projected_columns = vec![
      contract.geometry_column().to_string(),
      code_path.clone(),
      x_path.clone(),
      y_path.clone(),
    ];
    if let Some(path) = &z_path {
      projected_columns.push(path.clone());
    }
    if let Some(path) = &m_path {
      projected_columns.push(path.clone());
    }
    let mut previous_code = None;
    let mut minimum = None::<u64>;
    let mut maximum = None::<u64>;
    let mut sampled_geometry_count = 0usize;
    let read_result = file.read_row_groups(&projected_columns, |row_group, row_offset, batch| {
      let geometry = FileValidator::array_at_path(batch, contract.geometry_column())?;
      let x_values = FileValidator::float64_array_at_path(batch, &x_path)?;
      let y_values = FileValidator::float64_array_at_path(batch, &y_path)?;
      let code_values = FileValidator::uint64_array_at_path(batch, &code_path)?;
      let z_values = z_path
        .as_deref()
        .map(|path| FileValidator::float64_array_at_path(batch, path))
        .transpose()?;
      let m_values = m_path
        .as_deref()
        .map(|path| FileValidator::float64_array_at_path(batch, path))
        .transpose()?;

      for row_index in 0..batch.num_rows() {
        let row = row_offset + row_index as u64;
        let code_location = ValidationLocation::file(file.file.relative_path.clone())
          .with_row_group(row_group)
          .with_row(row)
          .with_column(code_path.clone());
        let code =
          (!code_values.is_null(row_index)).then(|| -> u64 { code_values.value(row_index) });
        if let Some(code) = code {
          if previous_code.is_some_and(|previous| -> bool { previous > code }) {
            report.push(
              ValidationRule::ZOrder,
              ValidationSeverity::Error,
              code_location.clone(),
              format!(
                "Z code {code} is lower than preceding code {}",
                previous_code.expect("checked as present")
              ),
            );
          }
          previous_code = Some(code);
          minimum = Some(minimum.map_or(code, |value| -> u64 { value.min(code) }));
          maximum = Some(maximum.map_or(code, |value| -> u64 { value.max(code) }));
        }

        let x = (!x_values.is_null(row_index)).then(|| x_values.value(row_index));
        let y = (!y_values.is_null(row_index)).then(|| y_values.value(row_index));
        let z = z_values.as_ref().and_then(|values| -> Option<f64> {
          (!values.is_null(row_index)).then(|| -> f64 { values.value(row_index) })
        });
        let m = m_values.as_ref().and_then(|values| -> Option<f64> {
          (!values.is_null(row_index)).then(|| -> f64 { values.value(row_index) })
        });
        let coordinate_location = ValidationLocation::file(file.file.relative_path.clone())
          .with_row_group(row_group)
          .with_row(row)
          .with_column(format!("{x_path},{y_path}"));
        if geometry.is_null(row_index) {
          continue;
        }

        let coordinates = x
          .zip(y)
          .filter(|(x, y)| -> bool { x.is_finite() && y.is_finite() });
        let indeterminate_component = (index.has_z
          && z.is_none_or(|value| -> bool { !value.is_finite() }))
          || (index.has_m && m.is_none_or(|value| -> bool { !value.is_finite() }));
        // Omitting fullExtent claims that every source location is indeterminate.
        // Verify that claim beyond the ordinary geometry sample.
        if sampled_geometry_count >= 4
          && index.full_extent.is_some()
          && coordinates.is_some()
          && !indeterminate_component
          && code.is_some()
        {
          continue;
        }
        sampled_geometry_count += 1;
        let bytes = match GeometryValidator::binary_value(geometry.as_ref(), row_index) {
          Ok(Some(bytes)) => bytes,
          Ok(None) => continue,
          Err(error) => {
            report.push(
              ValidationRule::Geometry,
              ValidationSeverity::Error,
              code_location.clone(),
              error.to_string(),
            );
            continue;
          }
        };
        let geometry_location = ValidationLocation::file(file.file.relative_path.clone())
          .with_row_group(row_group)
          .with_row(row)
          .with_column(contract.geometry_column().to_string());
        let inspection = match GeometryValidator::inspect(&bytes) {
          Ok(inspection) => inspection,
          Err(error) => {
            report.push(
              ValidationRule::Geometry,
              ValidationSeverity::Error,
              geometry_location,
              format!("invalid sampled WKB geometry: {error}"),
            );
            continue;
          }
        };
        GeometryValidator::validate(
          &inspection,
          index.geometry_type,
          index.has_z,
          index.has_m,
          geometry_location,
          report,
        );
        if inspection.extent.is_none() {
          if coordinates.is_some() {
            report.push(
              ValidationRule::ZCoordinate,
              ValidationSeverity::Error,
              coordinate_location,
              "indeterminate source geometry must not advertise a finite XY location",
            );
          }
          continue;
        }
        if code.is_none() {
          report.push(
            ValidationRule::ZCode,
            ValidationSeverity::Error,
            code_location.clone(),
            "determinate source geometry requires a non-null Z code",
          );
        }
        let full_extent = index.full_extent;
        if full_extent.is_none() {
          report.push(
            ValidationRule::Extent,
            ValidationSeverity::Error,
            code_location.clone().with_column("geodisplay.fullExtent"),
            "fullExtent is required when a source geometry has a finite XY location",
          );
        }
        let Ok(geometry_coordinate) = Coord::from_point_wkb(&bytes) else {
          continue;
        };
        let geometry_x = geometry_coordinate.x;
        let geometry_y = geometry_coordinate.y;
        let Some((x, y)) = coordinates else {
          report.push(
            ValidationRule::ZCoordinate,
            ValidationSeverity::Error,
            coordinate_location,
            "determinate source geometry requires finite non-null x and y values",
          );
          continue;
        };
        if !x.is_finite()
          || !y.is_finite()
          || !MetadataValidator::float_matches(x, geometry_x)
          || !MetadataValidator::float_matches(y, geometry_y)
        {
          report.push(
            ValidationRule::ZCoordinate,
            ValidationSeverity::Error,
            coordinate_location,
            format!(
              "stored x/y ({x}, {y}) do not match sampled WKB point ({geometry_x}, {geometry_y})"
            ),
          );
        }
        Self::validate_point_component(
          geometry_coordinate.z,
          z,
          z_path.as_deref(),
          row_group,
          row,
          file,
          report,
        );
        Self::validate_point_component(
          geometry_coordinate.m,
          m,
          m_path.as_deref(),
          row_group,
          row,
          file,
          report,
        );
        let Some(full_extent) = full_extent else {
          continue;
        };
        let Some(code) = code else {
          continue;
        };
        let expected_code = ClusterKey::from_z_coordinates(
          full_extent,
          geometry_x,
          geometry_y,
          index.coordinate_precision,
        )
        .value();
        if code != expected_code {
          report.push(
            ValidationRule::ZCode,
            ValidationSeverity::Error,
            code_location,
            format!("stored Z code {code} does not match recomputed code {expected_code}"),
          );
        }
      }
      Ok(())
    });
    if let Err(error) = read_result {
      report.push(
        ValidationRule::RowGroup,
        ValidationSeverity::Error,
        ValidationLocation::file(file.file.relative_path.clone()),
        error.to_string(),
      );
    }

    match (minimum, maximum) {
      (Some(minimum), Some(maximum)) => Some(ClusteringRange {
        file: file.file.relative_path.clone(),
        family: PartitionFamily::Z,
        minimum,
        maximum,
        partition: file.file.partition,
      }),
      _ => None,
    }
  }

  /// Require optional Z/M field presence to agree with its metadata dimension flag.
  fn validate_dimension_field(
    file: &FileValidator,
    path: Option<String>,
    required: bool,
    metadata_name: &str,
    report: &mut ValidationReport,
  ) {
    match (path, required) {
      (Some(path), true) => Self::validate_field(file, &path, &DataType::Float64, report),
      (Some(_), false) => report.push(
        ValidationRule::ZSchema,
        ValidationSeverity::Error,
        ValidationLocation::file(file.file.relative_path.clone())
          .with_column(format!("geodisplay.{metadata_name}")),
        format!("{metadata_name} must be absent when its dimension flag is false"),
      ),
      (None, true) => report.push(
        ValidationRule::ZSchema,
        ValidationSeverity::Error,
        ValidationLocation::file(file.file.relative_path.clone())
          .with_column(format!("geodisplay.{metadata_name}")),
        format!("{metadata_name} is required when its dimension flag is true"),
      ),
      (None, false) => {}
    }
  }

  /// Require the declared Arrow type; value validation determines where nulls are permitted.
  fn validate_field(
    file: &FileValidator,
    path: &str,
    expected_type: &DataType,
    report: &mut ValidationReport,
  ) {
    let location =
      ValidationLocation::file(file.file.relative_path.clone()).with_column(path.to_string());
    match FileValidator::field_at_path(file.metadata.schema().as_ref(), path) {
      Some(field) if field.data_type() == expected_type => {}
      Some(field) => report.push(
        ValidationRule::ZSchema,
        ValidationSeverity::Error,
        location,
        format!(
          "Z column must be {expected_type}, found {}",
          field.data_type(),
        ),
      ),
      None => report.push(
        ValidationRule::ZSchema,
        ValidationSeverity::Error,
        location,
        "required Z column is missing",
      ),
    }
  }

  /// Require finite source Z/M values to match; accept null or non-finite indeterminate components.
  fn validate_point_component(
    geometry_value: Option<f64>,
    column_value: Option<f64>,
    column_path: Option<&str>,
    row_group: usize,
    row: u64,
    file: &FileValidator,
    report: &mut ValidationReport,
  ) {
    let (Some(geometry_value), Some(column_path)) = (geometry_value, column_path) else {
      return;
    };
    let matches = match column_value {
      Some(column_value) if geometry_value.is_finite() => {
        column_value.is_finite() && MetadataValidator::float_matches(geometry_value, column_value)
      }
      Some(column_value) => !column_value.is_finite(),
      None => !geometry_value.is_finite(),
    };
    if matches {
      return;
    }
    report.push(
      ValidationRule::ZCoordinate,
      ValidationSeverity::Error,
      ValidationLocation::file(file.file.relative_path.clone())
        .with_row_group(row_group)
        .with_row(row)
        .with_column(column_path.to_string()),
      format!(
        "stored coordinate value {column_value:?} does not match sampled WKB coordinate value {geometry_value}"
      ),
    );
  }
}

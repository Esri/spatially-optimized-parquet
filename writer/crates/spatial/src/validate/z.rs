use arrow_array::{Array, Float64Array, UInt64Array};
use arrow_schema::DataType;

use crate::optimized::{point_xy_from_wkb, point_z_code};
use crate::output::ZClusteringIndex;
use crate::parquet_dataset::PartitionFamily;

use super::geometry::{binary_value, inspect_wkb_geometry, validate_geometry_inspection};
use super::metadata::{ValidatedMetadata, float_matches};
use super::multifile::FileCodeRange;
use super::report::{ValidationLocation, ValidationReport, ValidationRule, ValidationSeverity};
use super::structure::{LoadedDatasetFile, array_at_path, field_at_path, read_row_groups};

pub(crate) fn validate_z_schema(
  file: &LoadedDatasetFile,
  index: &ZClusteringIndex,
  report: &mut ValidationReport,
) {
  validate_required_field(file, &index.code, &DataType::UInt64, false, report);
  validate_required_field(file, &index.x_column, &DataType::Float64, false, report);
  validate_required_field(file, &index.y_column, &DataType::Float64, false, report);
  validate_dimension_field(
    file,
    index.z_column.as_deref(),
    index.has_z,
    "zColumn",
    report,
  );
  validate_dimension_field(
    file,
    index.m_column.as_deref(),
    index.has_m,
    "mColumn",
    report,
  );
}

fn validate_required_field(
  file: &LoadedDatasetFile,
  path: &str,
  expected_type: &DataType,
  nullable: bool,
  report: &mut ValidationReport,
) {
  let location =
    ValidationLocation::file(file.file.relative_path.clone()).with_column(path.to_string());
  match field_at_path(file.metadata.schema().as_ref(), path) {
    Some(field) if field.data_type() == expected_type && field.is_nullable() == nullable => {}
    Some(field) => report.push(
      ValidationRule::ZSchema,
      ValidationSeverity::Error,
      location,
      format!(
        "Z column must be {expected_type} with nullable={nullable}, found {} with nullable={}",
        field.data_type(),
        field.is_nullable()
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

fn validate_dimension_field(
  file: &LoadedDatasetFile,
  path: Option<&str>,
  required: bool,
  metadata_name: &str,
  report: &mut ValidationReport,
) {
  match (path, required) {
    (Some(path), true) => validate_required_field(file, path, &DataType::Float64, false, report),
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

pub(crate) fn validate_z_file(
  file: &LoadedDatasetFile,
  contract: &ValidatedMetadata,
  index: &ZClusteringIndex,
  report: &mut ValidationReport,
) -> Option<FileCodeRange> {
  let mut previous_code = None;
  let mut minimum = None::<u64>;
  let mut maximum = None::<u64>;
  let mut sampled_geometry_count = 0usize;
  let read_result = read_row_groups(file, |row_group, row_offset, batch| {
    let Ok(geometry) = array_at_path(batch, contract.geometry_column()) else {
      return;
    };
    let Ok(x_values) = array_at_path(batch, &index.x_column) else {
      return;
    };
    let Ok(y_values) = array_at_path(batch, &index.y_column) else {
      return;
    };
    let Ok(code_values) = array_at_path(batch, &index.code) else {
      return;
    };
    let Some(x_values) = x_values.as_any().downcast_ref::<Float64Array>() else {
      return;
    };
    let Some(y_values) = y_values.as_any().downcast_ref::<Float64Array>() else {
      return;
    };
    let Some(code_values) = code_values.as_any().downcast_ref::<UInt64Array>() else {
      return;
    };

    for row_index in 0..batch.num_rows() {
      let row = row_offset + row_index as u64;
      let code_location = ValidationLocation::file(file.file.relative_path.clone())
        .with_row_group(row_group)
        .with_row(row)
        .with_column(index.code.clone());
      if code_values.is_null(row_index) {
        report.push(
          ValidationRule::ZCode,
          ValidationSeverity::Error,
          code_location,
          "Z code must not be null",
        );
        continue;
      }
      let code = code_values.value(row_index);
      if previous_code.is_some_and(|previous| previous > code) {
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
      minimum = Some(minimum.map_or(code, |value| value.min(code)));
      maximum = Some(maximum.map_or(code, |value| value.max(code)));

      let x = (!x_values.is_null(row_index)).then(|| x_values.value(row_index));
      let y = (!y_values.is_null(row_index)).then(|| y_values.value(row_index));
      let coordinate_location = ValidationLocation::file(file.file.relative_path.clone())
        .with_row_group(row_group)
        .with_row(row)
        .with_column(format!("{},{}", index.x_column, index.y_column));
      let coordinates = match (x, y) {
        (Some(x), Some(y)) => Some((x, y)),
        _ => {
          report.push(
            ValidationRule::ZCoordinate,
            ValidationSeverity::Error,
            coordinate_location.clone(),
            "required point coordinate columns contain null",
          );
          None
        }
      };

      if geometry.is_null(row_index) {
        if coordinates.is_some_and(|(x, y)| !x.is_nan() || !y.is_nan()) {
          report.push(
            ValidationRule::ZCoordinate,
            ValidationSeverity::Error,
            coordinate_location.clone(),
            "null geometry requires NaN x and y values",
          );
        }
        if code != 0 {
          report.push(
            ValidationRule::ZCode,
            ValidationSeverity::Error,
            code_location,
            "null geometry requires Z code 0",
          );
        }
        continue;
      }

      if sampled_geometry_count >= 4 {
        continue;
      }
      sampled_geometry_count += 1;
      let bytes = match binary_value(geometry, row_index) {
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
      let inspection = match inspect_wkb_geometry(&bytes) {
        Ok(inspection) => inspection,
        Err(error) => {
          report.push(
            ValidationRule::Geometry,
            ValidationSeverity::Error,
            geometry_location,
            format!("invalid sampled WKB geometry: {error}"),
          );
          if coordinates.is_some_and(|(x, y)| !x.is_nan() || !y.is_nan()) {
            report.push(
              ValidationRule::ZCoordinate,
              ValidationSeverity::Error,
              coordinate_location,
              "invalid geometry requires NaN x and y values",
            );
          }
          continue;
        }
      };
      validate_geometry_inspection(&inspection, "point", geometry_location, report);
      let Ok((geometry_x, geometry_y)) = point_xy_from_wkb(&bytes) else {
        continue;
      };
      let Some((x, y)) = coordinates else {
        continue;
      };
      if !x.is_finite()
        || !y.is_finite()
        || !float_matches(x, geometry_x)
        || !float_matches(y, geometry_y)
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
      let expected_code = point_z_code(
        index.full_extent,
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
    (Some(minimum), Some(maximum)) => Some(FileCodeRange {
      file: file.file.relative_path.clone(),
      family: PartitionFamily::Z,
      minimum,
      maximum,
      partition: file.file.partition,
    }),
    _ => None,
  }
}

use arrow_array::Array;
use arrow_schema::DataType;

use crate::geometry::WkbCoordinate;
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
  pub(crate) fn validate_schema(
    index: &ClusteringIndexZ,
    file: &FileValidator,
    parent_column: Option<&str>,
    report: &mut ValidationReport,
  ) {
    Self::validate_required_field(
      file,
      &FileValidator::display_column_path(parent_column, &index.code),
      &DataType::UInt64,
      false,
      report,
    );
    Self::validate_required_field(
      file,
      &FileValidator::display_column_path(parent_column, &index.x_column),
      &DataType::Float64,
      false,
      report,
    );
    Self::validate_required_field(
      file,
      &FileValidator::display_column_path(parent_column, &index.y_column),
      &DataType::Float64,
      false,
      report,
    );
    Self::validate_dimension_field(
      file,
      index
        .z_column
        .as_deref()
        .map(|column| FileValidator::display_column_path(parent_column, column)),
      index.has_z,
      "zColumn",
      report,
    );
    Self::validate_dimension_field(
      file,
      index
        .m_column
        .as_deref()
        .map(|column| FileValidator::display_column_path(parent_column, column)),
      index.has_m,
      "mColumn",
      report,
    );
  }

  fn validate_required_field(
    file: &FileValidator,
    path: &str,
    expected_type: &DataType,
    nullable: bool,
    report: &mut ValidationReport,
  ) {
    let location =
      ValidationLocation::file(file.file.relative_path.clone()).with_column(path.to_string());
    match FileValidator::field_at_path(file.metadata.schema().as_ref(), path) {
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
    file: &FileValidator,
    path: Option<String>,
    required: bool,
    metadata_name: &str,
    report: &mut ValidationReport,
  ) {
    match (path, required) {
      (Some(path), true) => {
        Self::validate_required_field(file, &path, &DataType::Float64, false, report)
      }
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

  pub(crate) fn validate_file(
    index: &ClusteringIndexZ,
    file: &FileValidator,
    contract: &ValidatedMetadata,
    parent_column: Option<&str>,
    report: &mut ValidationReport,
  ) -> Option<ClusteringRange> {
    let code_path = FileValidator::display_column_path(parent_column, &index.code);
    let x_path = FileValidator::display_column_path(parent_column, &index.x_column);
    let y_path = FileValidator::display_column_path(parent_column, &index.y_column);
    let z_path = index
      .z_column
      .as_deref()
      .map(|column| FileValidator::display_column_path(parent_column, column));
    let m_path = index
      .m_column
      .as_deref()
      .map(|column| FileValidator::display_column_path(parent_column, column));
    let projected_columns = [
      contract.geometry_column().to_string(),
      code_path.clone(),
      x_path.clone(),
      y_path.clone(),
    ]
    .into_iter()
    .chain(z_path.iter().cloned())
    .chain(m_path.iter().cloned())
    .collect::<Vec<_>>();
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
        .with_column(format!("{x_path},{y_path}"));
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
      let bytes = match GeometryValidator::binary_value(geometry, row_index) {
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
      GeometryValidator::validate(
        &inspection,
        "point",
        index.has_z,
        index.has_m,
        geometry_location,
        report,
      );
      let Ok(geometry_coordinate) = WkbCoordinate::from_point_wkb(&bytes) else {
        continue;
      };
      let geometry_x = geometry_coordinate.x;
      let geometry_y = geometry_coordinate.y;
      let Some((x, y)) = coordinates else {
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
      validate_point_component(
        geometry_coordinate.z,
        z_values.map(|values| values.value(row_index)),
        z_path.as_deref(),
        row_group,
        row,
        file,
        report,
      );
      validate_point_component(
        geometry_coordinate.m,
        m_values.map(|values| values.value(row_index)),
        m_path.as_deref(),
        row_group,
        row,
        file,
        report,
      );
      let expected_code = ClusterKey::from_z_coordinates(
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

      fn validate_point_component(
        geometry_value: Option<f64>,
        column_value: Option<f64>,
        column_path: Option<&str>,
        row_group: usize,
        row: u64,
        file: &FileValidator,
        report: &mut ValidationReport,
      ) {
        let (Some(geometry_value), Some(column_value), Some(column_path)) =
          (geometry_value, column_value, column_path)
        else {
          return;
        };
        if (geometry_value.is_nan() && column_value.is_nan())
          || MetadataValidator::float_matches(geometry_value, column_value)
        {
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
            "stored coordinate value {column_value} does not match sampled WKB coordinate value {geometry_value}"
          ),
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
}

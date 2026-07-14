use arrow_array::{Array, Float64Array, UInt64Array};
use arrow_schema::DataType;

use crate::geometry::Extent2D;
use crate::optimized::{decode_pbf_geometry, extent_xz_code};
use crate::output::XzClusteringIndex;
use crate::parquet_dataset::PartitionFamily;

use super::geometry::{binary_value, inspect_wkb_geometry, validate_geometry_inspection};
use super::metadata::{ValidatedMetadata, geometry_base_type};
use super::multifile::FileCodeRange;
use super::report::{ValidationLocation, ValidationReport, ValidationRule, ValidationSeverity};
use super::structure::{LoadedDatasetFile, array_at_path, field_at_path, read_row_groups};

const PBF_SEARCH_LIMIT: usize = 512;

pub(crate) fn validate_xz_schema(
  file: &LoadedDatasetFile,
  index: &XzClusteringIndex,
  report: &mut ValidationReport,
) {
  let field_location =
    ValidationLocation::file(file.file.relative_path.clone()).with_column(index.field.clone());
  match field_at_path(file.metadata.schema().as_ref(), &index.field) {
    Some(field) if matches!(field.data_type(), DataType::Struct(_)) => {}
    Some(field) => report.push(
      ValidationRule::XzSchema,
      ValidationSeverity::Error,
      field_location,
      format!(
        "XZ field must be an Arrow struct, found {}",
        field.data_type()
      ),
    ),
    None => report.push(
      ValidationRule::XzSchema,
      ValidationSeverity::Error,
      field_location,
      "XZ field is missing",
    ),
  }

  let code_path = format!("{}.{}", index.field, index.code);
  validate_xz_field(file, &code_path, &DataType::UInt64, Some(false), report);
  for level in &index.levels {
    let level_path = format!("{}.{}", index.field, level.column);
    let location =
      ValidationLocation::file(file.file.relative_path.clone()).with_column(level_path.clone());
    match field_at_path(file.metadata.schema().as_ref(), &level_path) {
      Some(field)
        if matches!(
          field.data_type(),
          DataType::Binary | DataType::LargeBinary | DataType::BinaryView
        ) => {}
      Some(field) => report.push(
        ValidationRule::XzSchema,
        ValidationSeverity::Error,
        location,
        format!(
          "multiscale column must use an Arrow binary type, found {}",
          field.data_type()
        ),
      ),
      None => report.push(
        ValidationRule::XzSchema,
        ValidationSeverity::Error,
        location,
        "multiscale column is missing",
      ),
    }
  }
}

fn validate_xz_field(
  file: &LoadedDatasetFile,
  path: &str,
  expected_type: &DataType,
  nullable: Option<bool>,
  report: &mut ValidationReport,
) {
  let location =
    ValidationLocation::file(file.file.relative_path.clone()).with_column(path.to_string());
  match field_at_path(file.metadata.schema().as_ref(), path) {
    Some(field)
      if field.data_type() == expected_type
        && nullable.is_none_or(|nullable| field.is_nullable() == nullable) => {}
    Some(field) => report.push(
      ValidationRule::XzSchema,
      ValidationSeverity::Error,
      location,
      format!(
        "XZ column must be {expected_type}, found {} with nullable={}",
        field.data_type(),
        field.is_nullable()
      ),
    ),
    None => report.push(
      ValidationRule::XzSchema,
      ValidationSeverity::Error,
      location,
      "required XZ column is missing",
    ),
  }
}

pub(crate) fn validate_xz_file(
  file: &LoadedDatasetFile,
  contract: &ValidatedMetadata,
  index: &XzClusteringIndex,
  report: &mut ValidationReport,
) -> Option<FileCodeRange> {
  let code_path = format!("{}.{}", index.field, index.code);
  let level_paths = index
    .levels
    .iter()
    .map(|level| format!("{}.{}", index.field, level.column))
    .collect::<Vec<_>>();
  let mut previous_code = None;
  let mut minimum = None::<u64>;
  let mut maximum = None::<u64>;
  let mut sampled_geometry_count = 0usize;
  let mut level_search_count = vec![0usize; index.levels.len()];
  let mut level_found_payload = vec![false; index.levels.len()];
  let mut level_winding_warned = vec![false; index.levels.len()];
  let single_polygon = contract
    .geometry_types()
    .iter()
    .all(|geometry_type| geometry_base_type(geometry_type) == "Polygon");

  let read_result = read_row_groups(file, |row_group, row_offset, batch| {
    let Ok(geometry) = array_at_path(batch, contract.geometry_column()) else {
      return;
    };
    let Ok(code_values) = array_at_path(batch, &code_path) else {
      return;
    };
    let Some(code_values) = code_values.as_any().downcast_ref::<UInt64Array>() else {
      return;
    };
    let level_values = level_paths
      .iter()
      .map(|path| array_at_path(batch, path).ok())
      .collect::<Vec<_>>();

    for row_index in 0..batch.num_rows() {
      let row = row_offset + row_index as u64;
      let code_location = ValidationLocation::file(file.file.relative_path.clone())
        .with_row_group(row_group)
        .with_row(row)
        .with_column(code_path.clone());
      if code_values.is_null(row_index) {
        report.push(
          ValidationRule::XzCode,
          ValidationSeverity::Error,
          code_location,
          "XZ code must not be null",
        );
        continue;
      }
      let code = code_values.value(row_index);
      if previous_code.is_some_and(|previous| previous > code) {
        report.push(
          ValidationRule::XzOrder,
          ValidationSeverity::Error,
          code_location.clone(),
          format!(
            "XZ code {code} is lower than preceding code {}",
            previous_code.expect("checked as present")
          ),
        );
      }
      previous_code = Some(code);
      minimum = Some(minimum.map_or(code, |value| value.min(code)));
      maximum = Some(maximum.map_or(code, |value| value.max(code)));

      let geometry_is_null = geometry.is_null(row_index);
      if geometry_is_null && code != 0 {
        report.push(
          ValidationRule::XzCode,
          ValidationSeverity::Error,
          code_location.clone(),
          "null geometry requires XZ code 0",
        );
      }

      for (level_index, _) in index.levels.iter().enumerate() {
        if level_search_count[level_index] >= PBF_SEARCH_LIMIT {
          continue;
        }
        level_search_count[level_index] += 1;
        let Some(level_array) = level_values[level_index] else {
          continue;
        };
        let level_location = ValidationLocation::file(file.file.relative_path.clone())
          .with_row_group(row_group)
          .with_row(row)
          .with_column(level_paths[level_index].clone());
        let payload = match binary_value(level_array, row_index) {
          Ok(payload) => payload,
          Err(error) => {
            report.push(
              ValidationRule::Pbf,
              ValidationSeverity::Error,
              level_location,
              error.to_string(),
            );
            continue;
          }
        };
        match (geometry_is_null, payload) {
          (false, None) => report.push(
            ValidationRule::Pbf,
            ValidationSeverity::Error,
            level_location,
            "non-null source geometry requires a non-null multiscale payload",
          ),
          (true, None) => {}
          (source_is_null, Some(payload)) => {
            let decoded = match decode_pbf_geometry(&payload) {
              Ok(decoded) => decoded,
              Err(error) => {
                report.push(
                  ValidationRule::Pbf,
                  ValidationSeverity::Error,
                  level_location,
                  format!("invalid Esri PBF geometry: {error}"),
                );
                continue;
              }
            };
            let non_empty = validate_pbf_structure(
              &decoded.lengths,
              &decoded.coords,
              source_is_null,
              &level_location,
              report,
            );
            level_found_payload[level_index] |= non_empty;
            if non_empty && single_polygon && !level_winding_warned[level_index] {
              level_winding_warned[level_index] =
                warn_pbf_winding(&decoded.lengths, &decoded.coords, &level_location, report);
            }
          }
        }
      }

      if geometry_is_null || sampled_geometry_count >= 4 {
        continue;
      }
      sampled_geometry_count += 1;
      let geometry_location = ValidationLocation::file(file.file.relative_path.clone())
        .with_row_group(row_group)
        .with_row(row)
        .with_column(contract.geometry_column().to_string());
      let bytes = match binary_value(geometry, row_index) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => continue,
        Err(error) => {
          report.push(
            ValidationRule::Geometry,
            ValidationSeverity::Error,
            geometry_location,
            error.to_string(),
          );
          continue;
        }
      };
      let inspection = match inspect_wkb_geometry(&bytes) {
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
      validate_geometry_inspection(
        &inspection,
        &index.geometry_type,
        geometry_location.clone(),
        report,
      );
      let feature_extent = match covering_extent(batch, contract, row_index) {
        Ok(Some(extent)) => Some(extent),
        Ok(None) => inspection.extent,
        Err(error) => {
          report.push(
            ValidationRule::Extent,
            ValidationSeverity::Error,
            geometry_location.clone(),
            error,
          );
          inspection.extent
        }
      };
      let Some(feature_extent) = feature_extent else {
        report.push(
          ValidationRule::Extent,
          ValidationSeverity::Error,
          geometry_location,
          "unable to resolve sampled feature extent",
        );
        continue;
      };
      let expected_code =
        extent_xz_code(index.full_extent, feature_extent, index.max_level).value();
      if code != expected_code {
        report.push(
          ValidationRule::XzCode,
          ValidationSeverity::Error,
          code_location,
          format!("stored XZ code {code} does not match recomputed code {expected_code}"),
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

  for (level_index, level) in index.levels.iter().enumerate() {
    if !level_found_payload[level_index] {
      report.push(
        ValidationRule::PbfSample,
        ValidationSeverity::Warning,
        ValidationLocation::file(file.file.relative_path.clone())
          .with_column(level_paths[level_index].clone()),
        format!(
          "no non-empty PBF geometry found within the first {} records for multiscale level {}",
          level_search_count[level_index], level.level
        ),
      );
    }
  }

  match (minimum, maximum) {
    (Some(minimum), Some(maximum)) => Some(FileCodeRange {
      file: file.file.relative_path.clone(),
      family: PartitionFamily::Xz,
      minimum,
      maximum,
      partition: file.file.partition,
    }),
    _ => None,
  }
}

fn covering_extent(
  batch: &arrow_array::RecordBatch,
  contract: &ValidatedMetadata,
  row_index: usize,
) -> Result<Option<Extent2D>, String> {
  let Some(column) = contract.geo.columns.get(contract.geometry_column()) else {
    return Ok(None);
  };
  let Some(covering) = &column.covering else {
    return Ok(None);
  };
  let paths = [
    covering.bbox.xmin.join("."),
    covering.bbox.ymin.join("."),
    covering.bbox.xmax.join("."),
    covering.bbox.ymax.join("."),
  ];
  let mut values = [0.0; 4];
  for (value, path) in values.iter_mut().zip(paths) {
    let array = array_at_path(batch, &path).map_err(|error| error.to_string())?;
    let array = array
      .as_any()
      .downcast_ref::<Float64Array>()
      .ok_or_else(|| format!("covering field '{path}' is not Float64"))?;
    if array.is_null(row_index) {
      return Err(format!(
        "covering field '{path}' is null for a non-null geometry"
      ));
    }
    *value = array.value(row_index);
  }
  let extent = Extent2D {
    xmin: values[0],
    ymin: values[1],
    xmax: values[2],
    ymax: values[3],
  };
  if [extent.xmin, extent.ymin, extent.xmax, extent.ymax]
    .iter()
    .any(|value| !value.is_finite())
    || extent.xmin > extent.xmax
    || extent.ymin > extent.ymax
  {
    return Err("covering extent must contain finite ordered bounds".to_string());
  }
  Ok(Some(extent))
}

fn validate_pbf_structure(
  lengths: &[u32],
  coords: &[i64],
  source_is_null: bool,
  location: &ValidationLocation,
  report: &mut ValidationReport,
) -> bool {
  let coordinate_pairs = lengths
    .iter()
    .try_fold(0usize, |total, length| total.checked_add(*length as usize));
  let valid_width = coords.len() % 2 == 0;
  let lengths_match = coordinate_pairs
    .and_then(|pairs| pairs.checked_mul(2))
    .is_some_and(|coordinate_count| coordinate_count == coords.len());
  if !valid_width || !lengths_match || lengths.iter().any(|length| *length == 0) {
    report.push(
      ValidationRule::Pbf,
      ValidationSeverity::Error,
      location.clone(),
      "PBF lengths must be non-zero and account for every x/y coordinate pair",
    );
    return false;
  }
  let non_empty = !lengths.is_empty() && !coords.is_empty();
  if source_is_null && non_empty {
    report.push(
      ValidationRule::Pbf,
      ValidationSeverity::Error,
      location.clone(),
      "null source geometry must not contain a non-empty PBF payload",
    );
  } else if !source_is_null && !non_empty {
    report.push(
      ValidationRule::PbfDegenerate,
      ValidationSeverity::Error,
      location.clone(),
      "non-null source geometry must retain at least one PBF coordinate",
    );
  }
  if lengths == [1] && coords.len() != 2 {
    report.push(
      ValidationRule::PbfDegenerate,
      ValidationSeverity::Error,
      location.clone(),
      "degenerated PBF geometry must contain one x/y coordinate",
    );
  }
  non_empty
}

fn warn_pbf_winding(
  lengths: &[u32],
  coords: &[i64],
  location: &ValidationLocation,
  report: &mut ValidationReport,
) -> bool {
  let mut coordinate_offset = 0usize;
  let mut warned = false;
  for (part_index, length) in lengths.iter().copied().enumerate() {
    let length = length as usize;
    if length < 3 {
      coordinate_offset += length * 2;
      continue;
    }
    let mut absolute = Vec::with_capacity(length);
    let mut x = coords[coordinate_offset];
    let mut y = coords[coordinate_offset + 1];
    absolute.push((x, y));
    coordinate_offset += 2;
    for _ in 1..length {
      let Some(next_x) = x.checked_add(coords[coordinate_offset]) else {
        report.push(
          ValidationRule::Pbf,
          ValidationSeverity::Error,
          location.clone(),
          "PBF x-coordinate delta overflows i64",
        );
        return warned;
      };
      let Some(next_y) = y.checked_add(coords[coordinate_offset + 1]) else {
        report.push(
          ValidationRule::Pbf,
          ValidationSeverity::Error,
          location.clone(),
          "PBF y-coordinate delta overflows i64",
        );
        return warned;
      };
      x = next_x;
      y = next_y;
      absolute.push((x, y));
      coordinate_offset += 2;
    }
    let area = signed_area(&absolute);
    if area == 0 {
      continue;
    }
    let exterior = part_index == 0;
    if (exterior && area > 0) || (!exterior && area < 0) {
      report.push(
        ValidationRule::PbfWinding,
        ValidationSeverity::Warning,
        location.clone(),
        format!(
          "non-degenerate PBF {} ring has unexpected winding",
          if exterior { "exterior" } else { "interior" }
        ),
      );
      warned = true;
    }
  }
  warned
}

fn signed_area(coordinates: &[(i64, i64)]) -> i128 {
  if coordinates.len() < 3 {
    return 0;
  }
  let mut twice_area = 0_i128;
  for pair in coordinates.windows(2) {
    twice_area += pair[0].0 as i128 * pair[1].1 as i128 - pair[1].0 as i128 * pair[0].1 as i128;
  }
  if coordinates.first() != coordinates.last() {
    let first = coordinates[0];
    let last = coordinates[coordinates.len() - 1];
    twice_area += last.0 as i128 * first.1 as i128 - first.0 as i128 * last.1 as i128;
  }
  twice_area
}

#[cfg(test)]
mod tests {
  use std::path::PathBuf;

  use super::*;

  #[test]
  fn accepts_degenerated_pbf_and_rejects_empty_non_null_payload() {
    let location = ValidationLocation::file("data.parquet").with_column("sop.level_0");
    let mut report = ValidationReport::new(PathBuf::from("dataset"));

    assert!(validate_pbf_structure(
      &[1],
      &[10, 20],
      false,
      &location,
      &mut report
    ));
    assert!(!validate_pbf_structure(
      &[],
      &[],
      false,
      &location,
      &mut report
    ));

    assert_eq!(report.error_count(), 1);
    assert_eq!(report.findings()[0].rule(), ValidationRule::PbfDegenerate);
  }

  #[test]
  fn warns_for_counterclockwise_pbf_exterior() {
    let location = ValidationLocation::file("data.parquet").with_column("sop.level_0");
    let mut report = ValidationReport::new(PathBuf::from("dataset"));

    let warned = warn_pbf_winding(&[4], &[0, 0, 1, 0, 0, 1, -1, -1], &location, &mut report);

    assert!(warned);
    assert_eq!(report.warning_count(), 1);
    assert_eq!(report.findings()[0].rule(), ValidationRule::PbfWinding);
  }

  #[test]
  fn reports_pbf_delta_overflow_without_panicking() {
    let location = ValidationLocation::file("data.parquet").with_column("sop.level_0");
    let mut report = ValidationReport::new(PathBuf::from("dataset"));

    let warned = warn_pbf_winding(&[3], &[i64::MAX, 0, 1, 0, 0, 1], &location, &mut report);

    assert!(!warned);
    assert_eq!(report.error_count(), 1);
    assert_eq!(report.findings()[0].rule(), ValidationRule::Pbf);
  }
}

use arrow_array::{Array, Float64Array};
use arrow_schema::DataType;

use crate::geometry::{
  Extent2D, GeometryType, WkbCoordinate, decode_pbf_geometry, native_geometry_data_type,
};
use crate::optimized::{
  GeometryPartRole, GeometryPartSink, extent_xz_code, visit_wkb_geometry_for_display,
};
use crate::optimized::{QUANTIZED_NATIVE_ENCODING, XzClusteringIndex};
use crate::parquet_dataset::PartitionFamily;

use super::geometry::{binary_value, inspect_wkb_geometry, validate_geometry_inspection};
use super::metadata::{ValidatedMetadata, geometry_base_type};
use super::multifile::FileCodeRange;
use super::report::{ValidationLocation, ValidationReport, ValidationRule, ValidationSeverity};
use super::structure::{
  LoadedDatasetFile, array_at_path, display_column_path, field_at_path, read_row_groups,
  uint64_array_at_path,
};

const PBF_SEARCH_LIMIT: usize = 512;

pub(crate) fn validate_xz_schema(
  file: &LoadedDatasetFile,
  parent_column: Option<&str>,
  index: &XzClusteringIndex,
  report: &mut ValidationReport,
) {
  if let Some(parent_column) = parent_column {
    let field_location = ValidationLocation::file(file.file.relative_path.clone())
      .with_column(parent_column.to_string());
    match field_at_path(file.metadata.schema().as_ref(), parent_column) {
      Some(field) if matches!(field.data_type(), DataType::Struct(_)) => {}
      Some(field) => report.push(
        ValidationRule::XzSchema,
        ValidationSeverity::Error,
        field_location,
        format!(
          "XZ parent column must be an Arrow struct, found {}",
          field.data_type()
        ),
      ),
      None => report.push(
        ValidationRule::XzSchema,
        ValidationSeverity::Error,
        field_location,
        "XZ parent column is missing",
      ),
    }
  }

  let code_path = display_column_path(parent_column, &index.code);
  validate_xz_field(file, &code_path, &DataType::UInt64, Some(false), report);
  let native_geometry_type = optimized_geometry_type(&index.geometry_type);
  for level in &index.levels {
    let level_path = display_column_path(parent_column, &level.column);
    let location =
      ValidationLocation::file(file.file.relative_path.clone()).with_column(level_path.clone());
    match field_at_path(file.metadata.schema().as_ref(), &level_path) {
      Some(field)
        if index.encoding == QUANTIZED_NATIVE_ENCODING
          && native_geometry_type.is_some_and(|geometry_type| {
            field.data_type() == &native_geometry_data_type(geometry_type, index.has_z, index.has_m)
          }) => {}
      Some(field)
        if index.encoding != QUANTIZED_NATIVE_ENCODING
          && matches!(
            field.data_type(),
            DataType::Binary | DataType::LargeBinary | DataType::BinaryView
          ) => {}
      Some(field) => report.push(
        ValidationRule::XzSchema,
        ValidationSeverity::Error,
        location,
        format!(
          "multiscale column has invalid type for encoding {}, found {}",
          index.encoding,
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
  parent_column: Option<&str>,
  index: &XzClusteringIndex,
  report: &mut ValidationReport,
) -> Option<FileCodeRange> {
  let code_path = display_column_path(parent_column, &index.code);
  let level_paths = index
    .levels
    .iter()
    .map(|level| display_column_path(parent_column, &level.column))
    .collect::<Vec<_>>();
  let projected_columns = std::iter::once(contract.geometry_column().to_string())
    .chain(std::iter::once(code_path.clone()))
    .chain(level_paths.iter().cloned())
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
  let native_geometry = index.encoding == QUANTIZED_NATIVE_ENCODING;

  let read_result = read_row_groups(file, &projected_columns, |row_group, row_offset, batch| {
    let geometry = array_at_path(batch, contract.geometry_column())?;
    let code_values = uint64_array_at_path(batch, &code_path)?;
    let level_values = level_paths
      .iter()
      .map(|path| array_at_path(batch, path))
      .collect::<anyhow::Result<Vec<_>>>()?;
    let covering_view = covering_extent_view(batch, contract);

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
        let level_array = level_values[level_index];
        let level_location = ValidationLocation::file(file.file.relative_path.clone())
          .with_row_group(row_group)
          .with_row(row)
          .with_column(level_paths[level_index].clone());
        if native_geometry {
          match (geometry_is_null, level_array.is_null(row_index)) {
            (false, true) => report.push(
              ValidationRule::XzSchema,
              ValidationSeverity::Error,
              level_location,
              "non-null source geometry requires a non-null multiscale geometry",
            ),
            (true, false) => report.push(
              ValidationRule::XzSchema,
              ValidationSeverity::Error,
              level_location,
              "null source geometry requires a null multiscale geometry",
            ),
            (false, false) => level_found_payload[level_index] = true,
            (true, true) => {}
          }
          continue;
        }
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
              index.has_z,
              index.has_m,
              source_is_null,
              &level_location,
              report,
            );
            level_found_payload[level_index] |= non_empty;
            if non_empty && single_polygon && !level_winding_warned[level_index] {
              level_winding_warned[level_index] = warn_pbf_winding(
                &decoded.lengths,
                &decoded.coords,
                index.has_z,
                index.has_m,
                &level_location,
                report,
              );
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
        index.has_z,
        index.has_m,
        geometry_location.clone(),
        report,
      );
      let optimized_geometry_type = match index.geometry_type.as_str() {
        "multipoint" => GeometryType::MultiPoint,
        "polyline" => GeometryType::Polyline,
        "polygon" => GeometryType::Polygon,
        _ => continue,
      };
      if native_geometry {
        let feature_extent = match resolve_covering_extent(&covering_view, row_index) {
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
        validate_sampled_xz_code(
          feature_extent,
          code,
          index,
          code_location,
          geometry_location,
          report,
        );
        continue;
      }
      for (level_index, level) in index.levels.iter().enumerate() {
        let level_array = level_values[level_index];
        let Ok(Some(payload)) = binary_value(level_array, row_index) else {
          continue;
        };
        let Ok(decoded) = decode_pbf_geometry(&payload) else {
          continue;
        };
        let level_location = ValidationLocation::file(file.file.relative_path.clone())
          .with_row_group(row_group)
          .with_row(row)
          .with_column(level_paths[level_index].clone());
        validate_pbf_vertex_provenance(
          &bytes,
          optimized_geometry_type,
          &decoded.lengths,
          &decoded.coords,
          level,
          index.has_z,
          index.has_m,
          &level_location,
          report,
        );
      }
      let feature_extent = match resolve_covering_extent(&covering_view, row_index) {
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

  for (level_index, level) in index.levels.iter().enumerate() {
    if native_geometry {
      continue;
    }
    if !level_found_payload[level_index] {
      report.push(
        ValidationRule::PbfSample,
        ValidationSeverity::Warning,
        ValidationLocation::file(file.file.relative_path.clone())
          .with_column(level_paths[level_index].clone()),
        format!(
          "no non-empty Esri PBF geometry found within the first {} records for multiscale level {}",
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

fn optimized_geometry_type(value: &str) -> Option<GeometryType> {
  match value {
    "multipoint" => Some(GeometryType::MultiPoint),
    "polyline" => Some(GeometryType::Polyline),
    "polygon" => Some(GeometryType::Polygon),
    _ => None,
  }
}

fn validate_sampled_xz_code(
  feature_extent: Option<Extent2D>,
  code: u64,
  index: &XzClusteringIndex,
  code_location: ValidationLocation,
  geometry_location: ValidationLocation,
  report: &mut ValidationReport,
) {
  let Some(feature_extent) = feature_extent else {
    report.push(
      ValidationRule::Extent,
      ValidationSeverity::Error,
      geometry_location,
      "unable to resolve sampled feature extent",
    );
    return;
  };
  let expected_code = extent_xz_code(index.full_extent, feature_extent, index.max_level).value();
  if code != expected_code {
    report.push(
      ValidationRule::XzCode,
      ValidationSeverity::Error,
      code_location,
      format!("stored XZ code {code} does not match recomputed code {expected_code}"),
    );
  }
}

#[allow(clippy::too_many_arguments)]
fn validate_pbf_vertex_provenance(
  wkb: &[u8],
  geometry_type: GeometryType,
  lengths: &[u32],
  coords: &[i64],
  level: &crate::optimized::MultiscaleLevel,
  has_z: bool,
  has_m: bool,
  location: &ValidationLocation,
  report: &mut ValidationReport,
) {
  let stride = 2 + usize::from(has_z) + usize::from(has_m);
  let expected_coordinate_count = lengths
    .iter()
    .try_fold(0usize, |total, length| total.checked_add(*length as usize))
    .and_then(|count| count.checked_mul(stride));
  if expected_coordinate_count != Some(coords.len()) {
    return;
  }
  let mut collector = CoordinateCollector::default();
  if visit_wkb_geometry_for_display(wkb, geometry_type, &mut collector).is_err() {
    return;
  }
  let source_coordinates = collector
    .coordinates
    .into_iter()
    .filter_map(|coordinate| {
      Some((
        quantized_value(
          coordinate.x,
          level.transform.scale[0],
          level.transform.translate[0],
        )?,
        quantized_value(
          coordinate.y,
          level.transform.scale[1],
          level.transform.translate[1],
        )?,
        has_z.then(|| {
          quantized_component(
            coordinate.z,
            level.transform.scale[2],
            level.transform.translate[2],
          )
        }),
        has_m.then(|| {
          quantized_component(
            coordinate.m,
            level.transform.scale[3],
            level.transform.translate[3],
          )
        }),
      ))
    })
    .collect::<Vec<_>>();
  let mut offset = 0usize;
  for length in lengths {
    let mut x = 0i64;
    let mut y = 0i64;
    for vertex_index in 0..*length as usize {
      let encoded_x = coords[offset];
      let encoded_y = coords[offset + 1];
      if vertex_index == 0 {
        x = encoded_x;
        y = encoded_y;
      } else {
        let Some(next_x) = x.checked_add(encoded_x) else {
          return;
        };
        let Some(next_y) = y.checked_add(encoded_y) else {
          return;
        };
        x = next_x;
        y = next_y;
      }
      let mut dimension_offset = 2;
      let z = has_z.then(|| {
        let value = coords[offset + dimension_offset];
        dimension_offset += 1;
        value
      });
      let m = has_m.then(|| coords[offset + dimension_offset]);
      if !source_coordinates.contains(&(x, y, z, m)) {
        report.push(
          ValidationRule::Pbf,
          ValidationSeverity::Error,
          location.clone(),
          "Esri PBF coordinate does not match any quantized source WKB vertex",
        );
        return;
      }
      offset += stride;
    }
  }
}

fn quantized_component(value: Option<f64>, scale: f64, translate: f64) -> i64 {
  value
    .filter(|value| value.is_finite())
    .and_then(|value| quantized_value(value, scale, translate))
    .unwrap_or(0)
}

fn quantized_value(value: f64, scale: f64, translate: f64) -> Option<i64> {
  let quantized = ((value - translate) / scale).round();
  (quantized.is_finite() && quantized >= i64::MIN as f64 && quantized <= i64::MAX as f64)
    .then_some(quantized as i64)
}

#[derive(Default)]
struct CoordinateCollector {
  coordinates: Vec<WkbCoordinate>,
}

impl GeometryPartSink for CoordinateCollector {
  fn start_part(&mut self, _: GeometryPartRole) {}

  fn push_coord(&mut self, coordinate: WkbCoordinate) {
    self.coordinates.push(coordinate);
  }

  fn finish_part(&mut self) {}
}

struct CoveringExtentView<'array> {
  xmin: CoveringField<'array>,
  ymin: CoveringField<'array>,
  xmax: CoveringField<'array>,
  ymax: CoveringField<'array>,
}

struct CoveringField<'array> {
  array: &'array Float64Array,
  path: String,
}

fn covering_extent_view<'array>(
  batch: &'array arrow_array::RecordBatch,
  contract: &ValidatedMetadata,
) -> Result<Option<CoveringExtentView<'array>>, String> {
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
  let fields: [Result<CoveringField<'array>, String>; 4] = paths.map(|path| {
    let array = array_at_path(batch, &path).map_err(|error| error.to_string())?;
    let array = array
      .as_any()
      .downcast_ref::<Float64Array>()
      .ok_or_else(|| format!("covering field '{path}' is not Float64"))?;
    Ok(CoveringField { array, path })
  });
  let [xmin, ymin, xmax, ymax] = fields;
  Ok(Some(CoveringExtentView {
    xmin: xmin?,
    ymin: ymin?,
    xmax: xmax?,
    ymax: ymax?,
  }))
}

fn resolve_covering_extent(
  view: &Result<Option<CoveringExtentView<'_>>, String>,
  row_index: usize,
) -> Result<Option<Extent2D>, String> {
  let view = view.as_ref().map_err(Clone::clone)?;
  let Some(view) = view else {
    return Ok(None);
  };
  covering_extent(view, row_index)
}

fn covering_extent(
  view: &CoveringExtentView<'_>,
  row_index: usize,
) -> Result<Option<Extent2D>, String> {
  let values = [&view.xmin, &view.ymin, &view.xmax, &view.ymax];
  let mut extent_values = [0.0; 4];
  for (value, extent_value) in values.into_iter().zip(extent_values.iter_mut()) {
    if value.array.is_null(row_index) {
      return Err(format!(
        "covering field '{}' is null for a non-null geometry",
        value.path
      ));
    }
    *extent_value = value.array.value(row_index);
  }
  let extent = Extent2D {
    xmin: extent_values[0],
    ymin: extent_values[1],
    xmax: extent_values[2],
    ymax: extent_values[3],
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
  has_z: bool,
  has_m: bool,
  source_is_null: bool,
  location: &ValidationLocation,
  report: &mut ValidationReport,
) -> bool {
  let vertex_count = lengths
    .iter()
    .try_fold(0usize, |total, length| total.checked_add(*length as usize));
  let stride = 2 + usize::from(has_z) + usize::from(has_m);
  let valid_width = coords.len() % stride == 0;
  let lengths_match = vertex_count
    .and_then(|count| count.checked_mul(stride))
    .is_some_and(|coordinate_count| coordinate_count == coords.len());
  if !valid_width || !lengths_match || lengths.contains(&0) {
    report.push(
      ValidationRule::Pbf,
      ValidationSeverity::Error,
      location.clone(),
      format!(
        "Esri PBF lengths must be non-zero and account for every coordinate with stride {stride}"
      ),
    );
    return false;
  }
  let non_empty = !lengths.is_empty() && !coords.is_empty();
  if source_is_null && non_empty {
    report.push(
      ValidationRule::Pbf,
      ValidationSeverity::Error,
      location.clone(),
      "null source geometry must not contain a non-empty Esri PBF payload",
    );
  } else if !source_is_null && !non_empty {
    report.push(
      ValidationRule::PbfDegenerate,
      ValidationSeverity::Error,
      location.clone(),
      "non-null source geometry must retain at least one Esri PBF coordinate",
    );
  }
  if lengths == [1] && coords.len() != stride {
    report.push(
      ValidationRule::PbfDegenerate,
      ValidationSeverity::Error,
      location.clone(),
      "degenerated Esri PBF geometry must contain exactly one complete coordinate",
    );
  }
  non_empty
}

fn warn_pbf_winding(
  lengths: &[u32],
  coords: &[i64],
  has_z: bool,
  has_m: bool,
  location: &ValidationLocation,
  report: &mut ValidationReport,
) -> bool {
  let mut coordinate_offset = 0usize;
  let stride = 2 + usize::from(has_z) + usize::from(has_m);
  let mut warned = false;
  for (part_index, length) in lengths.iter().copied().enumerate() {
    let length = length as usize;
    if length < 3 {
      coordinate_offset += length * stride;
      continue;
    }
    let mut absolute = Vec::with_capacity(length);
    let mut x = coords[coordinate_offset];
    let mut y = coords[coordinate_offset + 1];
    absolute.push((x, y));
    coordinate_offset += stride;
    for _ in 1..length {
      let Some(next_x) = x.checked_add(coords[coordinate_offset]) else {
        report.push(
          ValidationRule::Pbf,
          ValidationSeverity::Error,
          location.clone(),
          "Esri PBF x-coordinate delta overflows i64",
        );
        return warned;
      };
      let Some(next_y) = y.checked_add(coords[coordinate_offset + 1]) else {
        report.push(
          ValidationRule::Pbf,
          ValidationSeverity::Error,
          location.clone(),
          "Esri PBF y-coordinate delta overflows i64",
        );
        return warned;
      };
      x = next_x;
      y = next_y;
      absolute.push((x, y));
      coordinate_offset += stride;
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
          "non-degenerate Esri PBF {} ring has unexpected winding",
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
  use crate::output::QuantizationTransform;

  #[test]
  fn accepts_degenerated_pbf_and_rejects_empty_non_null_payload() {
    let location = ValidationLocation::file("data.parquet").with_column("sop.level_0");
    let mut report = ValidationReport::new(PathBuf::from("dataset"));

    assert!(validate_pbf_structure(
      &[1],
      &[10, 20],
      false,
      false,
      false,
      &location,
      &mut report
    ));
    assert!(!validate_pbf_structure(
      &[],
      &[],
      false,
      false,
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

    let warned = warn_pbf_winding(
      &[4],
      &[0, 0, 1, 0, 0, 1, -1, -1],
      false,
      false,
      &location,
      &mut report,
    );

    assert!(warned);
    assert_eq!(report.warning_count(), 1);
    assert_eq!(report.findings()[0].rule(), ValidationRule::PbfWinding);
  }

  #[test]
  fn reports_pbf_delta_overflow_without_panicking() {
    let location = ValidationLocation::file("data.parquet").with_column("sop.level_0");
    let mut report = ValidationReport::new(PathBuf::from("dataset"));

    let warned = warn_pbf_winding(
      &[3],
      &[i64::MAX, 0, 1, 0, 0, 1],
      false,
      false,
      &location,
      &mut report,
    );

    assert!(!warned);
    assert_eq!(report.error_count(), 1);
    assert_eq!(report.findings()[0].rule(), ValidationRule::Pbf);
  }

  #[test]
  fn accepts_xyzm_coordinate_stride() {
    let location = ValidationLocation::file("data.parquet").with_column("sop.level_0");
    let mut report = ValidationReport::new(PathBuf::from("dataset"));

    assert!(validate_pbf_structure(
      &[2],
      &[0, 0, 10, 100, 1, 1, 20, 200],
      true,
      true,
      false,
      &location,
      &mut report,
    ));
    assert_eq!(report.error_count(), 0);
  }

  #[test]
  fn rejects_dimensional_pbf_component_not_present_in_wkb() {
    let mut wkb = vec![1];
    wkb.extend_from_slice(&1002_u32.to_le_bytes());
    wkb.extend_from_slice(&2_u32.to_le_bytes());
    for (x, y, z) in [(0.0_f64, 0.0_f64, 10.0_f64), (2.0, 0.0, 20.0)] {
      wkb.extend_from_slice(&x.to_le_bytes());
      wkb.extend_from_slice(&y.to_le_bytes());
      wkb.extend_from_slice(&z.to_le_bytes());
    }
    let level = crate::optimized::MultiscaleLevel {
      column: "level_0".to_string(),
      level: 0,
      resolution: 1.0,
      scale: 1.0,
      transform: QuantizationTransform {
        scale: [1.0; 4],
        translate: [0.0; 4],
      },
    };
    let location = ValidationLocation::file("data.parquet").with_column("sop.level_0");
    let mut report = ValidationReport::new(PathBuf::from("dataset"));

    validate_pbf_vertex_provenance(
      &wkb,
      GeometryType::Polyline,
      &[2],
      &[0, 0, 10, 2, 0, 999],
      &level,
      true,
      false,
      &location,
      &mut report,
    );

    assert_eq!(report.error_count(), 1);
    assert_eq!(report.findings()[0].rule(), ValidationRule::Pbf);
  }
}

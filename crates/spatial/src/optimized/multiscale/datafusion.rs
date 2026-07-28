//! Implements point and complex geometry geodisplay encoding.

use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use arrow_array::{Array, ArrayRef, Float64Array, StructArray};
use arrow_schema::{DataType, Field, FieldRef, Fields};
use datafusion::common::cast::as_float64_array;
use datafusion::common::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::{
  ColumnarValue, Expr, ReturnFieldArgs, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature,
  TypeSignature, Volatility,
};
use datafusion::prelude::col;

use super::MultiscaleEncoding;
use crate::geometry::{
  Geometry, GeometryArray, GeometryType, QuantizationOptions, QuantizedGeometry,
  to_datafusion_error,
};
use crate::pipeline::PipelineWarnings;

use super::{
  GEOLOD_COLUMN, MultiscaleLevel, POINT_M_COLUMN, POINT_X_COLUMN, POINT_Y_COLUMN, POINT_Z_COLUMN,
  SOP_GEOMETRY_COLUMN,
};

#[derive(Debug, Clone)]
/// Encodes complex geometry WKB into the complete geodisplay struct for each row.
///
/// Equality and hashing include every encoding parameter because DataFusion uses UDF
/// identity when comparing and optimizing logical expressions.
pub(crate) struct GeolodUdf {
  ty: GeometryType,
  has_z: bool,
  has_m: bool,
  levels: Vec<MultiscaleLevel>,
  multiscale_encoding: MultiscaleEncoding,
  geolod_fields: Fields,
  dimension_warning_emitted: Arc<AtomicBool>,
  warnings: PipelineWarnings,
}

impl GeolodUdf {
  /// Build the geodisplay expression for complex geometry output.
  pub(crate) fn expression(
    geometry_column: &str,
    geometry_type: GeometryType,
    has_z: bool,
    has_m: bool,
    levels: &[MultiscaleLevel],
    multiscale_encoding: MultiscaleEncoding,
    warnings: PipelineWarnings,
  ) -> Expr {
    Self::udf(
      geometry_type,
      has_z,
      has_m,
      levels.to_vec(),
      multiscale_encoding,
      warnings,
    )
    .call(vec![col(geometry_column)])
    .alias(GEOLOD_COLUMN)
  }

  /// Construct stable output fields for the selected geometry type and multiscale levels.
  #[cfg(test)]
  fn new(ty: GeometryType, has_z: bool, has_m: bool, levels: Vec<MultiscaleLevel>) -> Self {
    Self::new_with_warnings(
      ty,
      has_z,
      has_m,
      levels,
      MultiscaleEncoding::Pbf,
      PipelineWarnings::default(),
    )
  }

  fn new_with_warnings(
    ty: GeometryType,
    has_z: bool,
    has_m: bool,
    levels: Vec<MultiscaleLevel>,
    multiscale_encoding: MultiscaleEncoding,
    warnings: PipelineWarnings,
  ) -> Self {
    let mut geolod_fields = Vec::new();
    for encoding in &levels {
      geolod_fields.push(Arc::new(Field::new(
        &encoding.column,
        multiscale_encoding.geometry_data_type(ty, has_z, has_m),
        true,
      )));
    }

    Self {
      ty,
      has_z,
      has_m,
      levels,
      multiscale_encoding,
      geolod_fields: Fields::from(geolod_fields),
      dimension_warning_emitted: Arc::new(AtomicBool::new(false)),
      warnings,
    }
  }

  fn udf(
    geometry_type: GeometryType,
    has_z: bool,
    has_m: bool,
    levels: Vec<MultiscaleLevel>,
    multiscale_encoding: MultiscaleEncoding,
    warnings: PipelineWarnings,
  ) -> ScalarUDF {
    ScalarUDF::new_from_impl(Self::new_with_warnings(
      geometry_type,
      has_z,
      has_m,
      levels,
      multiscale_encoding,
      warnings,
    ))
  }

  fn signature() -> &'static Signature {
    static SIGNATURE: OnceLock<Signature> = OnceLock::new();
    SIGNATURE.get_or_init(|| {
      Signature::one_of(
        [
          DataType::Binary,
          DataType::LargeBinary,
          DataType::BinaryView,
        ]
        .into_iter()
        .map(|geometry_type| TypeSignature::Exact(vec![geometry_type]))
        .collect(),
        Volatility::Immutable,
      )
    })
  }

  /// Decode each non-null WKB value once and encode every configured multiscale level.
  fn encode_geolod(&self, geometry: &GeometryArray<'_>) -> DataFusionResult<StructArray> {
    let mut level_builders = self
      .levels
      .iter()
      .map(|_| {
        self
          .multiscale_encoding
          .resolve_writer(self.ty, self.has_z, self.has_m, geometry.len())
      })
      .collect::<Vec<_>>();
    let quantization_options = self
      .levels
      .iter()
      .map(|level| QuantizationOptions {
        transform: level.transform.clone(),
        tolerance: level.resolution,
        min_length: level.min_length,
        has_z: self.has_z,
        has_m: self.has_m,
      })
      .collect::<Vec<_>>();
    let mut quantized_geometry = QuantizedGeometry::default();

    for value in geometry.values() {
      match value {
        Some(bytes) => {
          let mut geometry = Geometry::from_wkb(bytes).map_err(to_datafusion_error)?;
          if geometry.ty != self.ty {
            return Err(DataFusionError::Execution(format!(
              "WKB geometry type {:?} does not match expected type {:?}",
              geometry.ty, self.ty
            )));
          }
          if self.multiscale_encoding == MultiscaleEncoding::Pbf && self.ty == GeometryType::Polygon
          {
            reverse_polygon_parts(&mut geometry);
          }
          let source_has_z = geometry
            .coordinates
            .iter()
            .any(|coordinate| coordinate.z.is_some());
          let source_has_m = geometry
            .coordinates
            .iter()
            .any(|coordinate| coordinate.m.is_some());
          if source_has_z != self.has_z || source_has_m != self.has_m {
            if !self.dimension_warning_emitted.swap(true, Ordering::Relaxed) {
              self.warnings.record(format!(
                "Warning: WKB dimensions do not match source metadata; \
                 normalizing multiscale geometry from hasZ={}, hasM={} to hasZ={}, hasM={} and \
                 encoding missing Z/M values as {}",
                source_has_z,
                source_has_m,
                self.has_z,
                self.has_m,
                self.multiscale_encoding.missing_component_value()
              ));
            }
          }
          for (writer, options) in level_builders.iter_mut().zip(&quantization_options) {
            quantized_geometry
              .quantize_from(&geometry, options)
              .map_err(to_datafusion_error)?;
            writer
              .append(&quantized_geometry)
              .map_err(to_datafusion_error)?;
          }
        }
        None => {
          for writer in &mut level_builders {
            writer.append_null();
          }
        }
      }
    }

    let mut geolod_columns = Vec::new();
    for builder in level_builders {
      geolod_columns.push(builder.finish());
    }

    StructArray::try_new(self.geolod_fields.clone(), geolod_columns, None)
      .map_err(to_datafusion_error)
  }
}

fn reverse_polygon_parts(geometry: &mut Geometry) {
  let mut part_start = 0;
  for part_length in &geometry.lengths {
    let part_end = part_start + *part_length as usize;
    geometry.coordinates[part_start..part_end].reverse();
    part_start = part_end;
  }
}

impl PartialEq for GeolodUdf {
  fn eq(&self, other: &Self) -> bool {
    self.ty == other.ty
      && self.has_z == other.has_z
      && self.has_m == other.has_m
      && self.multiscale_encoding == other.multiscale_encoding
      && self.levels.len() == other.levels.len()
      && self.levels.iter().zip(&other.levels).all(|(left, right)| {
        left.level == right.level
          && left.column == right.column
          && left.resolution.to_bits() == right.resolution.to_bits()
          && left.scale.to_bits() == right.scale.to_bits()
          && left
            .transform
            .scale
            .iter()
            .map(|value| value.to_bits())
            .eq(right.transform.scale.iter().map(|value| value.to_bits()))
          && left
            .transform
            .translate
            .iter()
            .map(|value| value.to_bits())
            .eq(
              right
                .transform
                .translate
                .iter()
                .map(|value| value.to_bits()),
            )
          && left.min_length == right.min_length
      })
  }
}

impl Eq for GeolodUdf {}

impl Hash for GeolodUdf {
  fn hash<H: Hasher>(&self, state: &mut H) {
    match self.ty {
      GeometryType::Point => 0u8,
      GeometryType::MultiPoint => 1u8,
      GeometryType::Polyline => 2u8,
      GeometryType::Polygon => 3u8,
    }
    .hash(state);
    self.has_z.hash(state);
    self.has_m.hash(state);
    self.multiscale_encoding.hash(state);
    self.levels.len().hash(state);
    for encoding in &self.levels {
      encoding.level.hash(state);
      encoding.column.hash(state);
      encoding.resolution.to_bits().hash(state);
      encoding.scale.to_bits().hash(state);
      for value in encoding.transform.scale {
        value.to_bits().hash(state);
      }
      for value in encoding.transform.translate {
        value.to_bits().hash(state);
      }
      encoding.min_length.hash(state);
    }
  }
}

impl ScalarUDFImpl for GeolodUdf {
  fn name(&self) -> &str {
    "geodisplay_complex_geometry"
  }

  fn signature(&self) -> &Signature {
    Self::signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::Struct(self.geolod_fields.clone()))
  }

  fn return_field_from_args(&self, _: ReturnFieldArgs) -> DataFusionResult<FieldRef> {
    Ok(Arc::new(Field::new(
      self.name(),
      DataType::Struct(self.geolod_fields.clone()),
      false,
    )))
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let geometry = arrays
      .first()
      .ok_or_else(|| DataFusionError::Execution("missing geometry argument".to_string()))?;
    let geometry = GeometryArray::try_new(geometry.as_ref())?;
    let output = self.encode_geolod(&geometry)?;

    Ok(ColumnarValue::Array(Arc::new(output) as ArrayRef))
  }
}

#[derive(Debug)]
/// Packs generated point index columns into the geodisplay struct.
pub(crate) struct SopGeometryUdf {
  has_z: bool,
  has_m: bool,
  signature: Signature,
}

impl SopGeometryUdf {
  /// Build the geodisplay expression for generated point index columns.
  pub(crate) fn expression(has_z: bool, has_m: bool) -> Expr {
    let mut arguments = vec![col(POINT_X_COLUMN), col(POINT_Y_COLUMN)];
    if has_z {
      arguments.push(col(POINT_Z_COLUMN));
    }
    if has_m {
      arguments.push(col(POINT_M_COLUMN));
    }
    ScalarUDF::new_from_impl(Self::new(has_z, has_m))
      .call(arguments)
      .alias(SOP_GEOMETRY_COLUMN)
  }

  fn new(has_z: bool, has_m: bool) -> Self {
    let mut types = vec![DataType::Float64, DataType::Float64];
    types.extend(std::iter::repeat_n(
      DataType::Float64,
      usize::from(has_z) + usize::from(has_m),
    ));
    Self {
      has_z,
      has_m,
      signature: Signature::exact(types, Volatility::Immutable),
    }
  }

  fn fields(has_z: bool, has_m: bool) -> Fields {
    let mut fields = vec![
      Arc::new(Field::new(POINT_X_COLUMN, DataType::Float64, false)),
      Arc::new(Field::new(POINT_Y_COLUMN, DataType::Float64, false)),
    ];
    if has_z {
      fields.push(Arc::new(Field::new(
        POINT_Z_COLUMN,
        DataType::Float64,
        false,
      )));
    }
    if has_m {
      fields.push(Arc::new(Field::new(
        POINT_M_COLUMN,
        DataType::Float64,
        false,
      )));
    }
    Fields::from(fields)
  }
}

impl PartialEq for SopGeometryUdf {
  fn eq(&self, other: &Self) -> bool {
    self.has_z == other.has_z && self.has_m == other.has_m
  }
}

impl Eq for SopGeometryUdf {}

impl Hash for SopGeometryUdf {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.has_z.hash(state);
    self.has_m.hash(state);
  }
}

impl ScalarUDFImpl for SopGeometryUdf {
  fn name(&self) -> &str {
    "sop_geometry"
  }

  fn signature(&self) -> &Signature {
    &self.signature
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::Struct(Self::fields(self.has_z, self.has_m)))
  }

  fn return_field_from_args(&self, _: ReturnFieldArgs) -> DataFusionResult<FieldRef> {
    Ok(Arc::new(Field::new(
      self.name(),
      DataType::Struct(Self::fields(self.has_z, self.has_m)),
      true,
    )))
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let x = as_float64_array(
      arrays
        .first()
        .ok_or_else(|| DataFusionError::Execution("missing x argument".to_string()))?
        .as_ref(),
    )?;
    let y = as_float64_array(
      arrays
        .get(1)
        .ok_or_else(|| DataFusionError::Execution("missing y argument".to_string()))?
        .as_ref(),
    )?;
    let nulls = x.nulls().cloned();
    let mut columns: Vec<ArrayRef> = vec![
      Arc::new(Float64Array::new(x.values().clone(), None)),
      Arc::new(Float64Array::new(y.values().clone(), None)),
    ];
    let mut argument_index = 2;
    if self.has_z {
      let z = as_float64_array(arrays[argument_index].as_ref())?;
      columns.push(Arc::new(Float64Array::new(z.values().clone(), None)));
      argument_index += 1;
    }
    if self.has_m {
      let m = as_float64_array(arrays[argument_index].as_ref())?;
      columns.push(Arc::new(Float64Array::new(m.values().clone(), None)));
    }
    let output = StructArray::try_new(Self::fields(self.has_z, self.has_m), columns, nulls)
      .map_err(to_datafusion_error)?;
    Ok(ColumnarValue::Array(Arc::new(output)))
  }
}

#[cfg(test)]
mod tests {
  use std::collections::hash_map::DefaultHasher;
  use std::hash::{Hash, Hasher};

  use arrow_array::{BinaryArray, Int64Array, ListArray};

  use crate::geometry::PbfGeometry;
  use crate::optimized::multiscale::MultiscaleLevel;

  use super::*;

  fn multiline_z_wkb() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.push(1);
    bytes.extend_from_slice(&1005_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.push(1);
    bytes.extend_from_slice(&1002_u32.to_le_bytes());
    bytes.extend_from_slice(&2_u32.to_le_bytes());
    for coordinate in [[1.0_f64, 2.0, 3.0], [4.0, 5.0, 6.0]] {
      for component in coordinate {
        bytes.extend_from_slice(&component.to_le_bytes());
      }
    }
    bytes
  }

  fn hash_udf(udf: &GeolodUdf) -> u64 {
    let mut hasher = DefaultHasher::new();
    udf.hash(&mut hasher);
    hasher.finish()
  }

  fn assert_encoding_parameter_changes_identity(mutate: impl FnOnce(&mut MultiscaleLevel)) {
    let levels = MultiscaleLevel::create_all(
      crate::geoparquet::DEFAULT_OUTPUT_WKID,
      GeometryType::Polygon,
    );
    let original = GeolodUdf::new(GeometryType::Polygon, false, false, levels.clone());
    let mut changed_levels = levels;
    mutate(&mut changed_levels[0]);
    let changed = GeolodUdf::new(GeometryType::Polygon, false, false, changed_levels);
    assert_ne!(original, changed);
  }

  #[test]
  fn identity_includes_every_encoding_parameter() {
    assert_encoding_parameter_changes_identity(|encoding| encoding.level += 1);
    assert_encoding_parameter_changes_identity(|encoding| encoding.column.push_str("_changed"));
    assert_encoding_parameter_changes_identity(|encoding| {
      encoding.resolution = f64::from_bits(encoding.resolution.to_bits() ^ 1)
    });
    assert_encoding_parameter_changes_identity(|encoding| {
      encoding.scale = f64::from_bits(encoding.scale.to_bits() ^ 1)
    });
    assert_encoding_parameter_changes_identity(|encoding| {
      encoding.transform.scale[0] = f64::from_bits(encoding.transform.scale[0].to_bits() ^ 1)
    });
    assert_encoding_parameter_changes_identity(|encoding| {
      encoding.transform.scale[1] = f64::from_bits(encoding.transform.scale[1].to_bits() ^ 1)
    });
    assert_encoding_parameter_changes_identity(|encoding| {
      encoding.transform.translate[0] =
        f64::from_bits(encoding.transform.translate[0].to_bits() ^ 1)
    });
    assert_encoding_parameter_changes_identity(|encoding| {
      encoding.transform.translate[1] =
        f64::from_bits(encoding.transform.translate[1].to_bits() ^ 1)
    });
    assert_encoding_parameter_changes_identity(|encoding| encoding.min_length += 1);
  }

  #[test]
  fn identity_includes_geometry_type_encoding_order_and_count() {
    let levels = MultiscaleLevel::create_all(
      crate::geoparquet::DEFAULT_OUTPUT_WKID,
      GeometryType::Polygon,
    );
    let original = GeolodUdf::new(GeometryType::Polygon, false, false, levels.clone());
    let equal = GeolodUdf::new(GeometryType::Polygon, false, false, levels.clone());
    assert_eq!(original, equal);
    assert_eq!(hash_udf(&original), hash_udf(&equal));

    let different_type = GeolodUdf::new(GeometryType::Polyline, false, false, levels.clone());
    assert_ne!(original, different_type);

    let mut reordered = levels.clone();
    reordered.swap(0, 1);
    assert_ne!(
      original,
      GeolodUdf::new(GeometryType::Polygon, false, false, reordered)
    );

    let mut shortened = levels;
    shortened.pop();
    assert_ne!(
      original,
      GeolodUdf::new(GeometryType::Polygon, false, false, shortened,)
    );
  }

  #[test]
  fn missing_m_values_are_encoded_as_zero_for_output_zm() {
    let levels = MultiscaleLevel::create_all(
      crate::geoparquet::DEFAULT_OUTPUT_WKID,
      GeometryType::Polyline,
    );
    let warnings = PipelineWarnings::default();
    let udf = GeolodUdf::new_with_warnings(
      GeometryType::Polyline,
      true,
      true,
      levels,
      MultiscaleEncoding::Pbf,
      warnings.clone(),
    );
    let wkb = multiline_z_wkb();
    let geometry = BinaryArray::from(vec![Some(wkb.as_slice())]);

    let geometry = GeometryArray::try_new(&geometry).expect("geometry");
    let geodisplay = udf.encode_geolod(&geometry).expect("geodisplay");
    let encoded = geodisplay
      .column(1)
      .as_any()
      .downcast_ref::<BinaryArray>()
      .expect("binary geometry");
    let decoded = PbfGeometry::from_bytes(encoded.value(0)).expect("decoded geometry");

    assert_eq!(decoded.coords.len(), 8);
    assert_eq!(decoded.coords[3], 0);
    assert_eq!(decoded.coords[7], 0);
    assert_eq!(warnings.messages().len(), 1);
  }

  #[test]
  fn missing_m_values_are_null_for_native_output_zm() {
    let levels = MultiscaleLevel::create_all(
      crate::geoparquet::DEFAULT_OUTPUT_WKID,
      GeometryType::Polyline,
    );
    let warnings = PipelineWarnings::default();
    let udf = GeolodUdf::new_with_warnings(
      GeometryType::Polyline,
      true,
      true,
      levels,
      MultiscaleEncoding::QuantizedNative,
      warnings.clone(),
    );
    let wkb = multiline_z_wkb();
    let geometry = BinaryArray::from(vec![Some(wkb.as_slice())]);

    let geometry = GeometryArray::try_new(&geometry).expect("geometry");
    let geodisplay = udf.encode_geolod(&geometry).expect("geodisplay");
    let geometries = geodisplay
      .column(1)
      .as_any()
      .downcast_ref::<ListArray>()
      .expect("native geometries");
    let parts = geometries
      .value(0)
      .as_any()
      .downcast_ref::<ListArray>()
      .expect("native parts")
      .clone();
    let coordinates = parts
      .value(0)
      .as_any()
      .downcast_ref::<StructArray>()
      .expect("native coordinates")
      .clone();
    let z = coordinates
      .column_by_name("z")
      .and_then(|column| column.as_any().downcast_ref::<Int64Array>())
      .expect("native z");
    let m = coordinates
      .column_by_name("m")
      .and_then(|column| column.as_any().downcast_ref::<Int64Array>())
      .expect("native m");

    assert_eq!(z.null_count(), 0);
    assert_eq!(m.null_count(), 2);
    assert!(warnings.messages()[0].contains("encoding missing Z/M values as null"));
  }
}

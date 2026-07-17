//! Implements point and complex geometry geodisplay encoding.

use std::any::Any;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use arrow_array::{Array, ArrayRef, Float64Array, StructArray, UInt64Array};
use arrow_schema::{DataType, Field, FieldRef, Fields};
use datafusion::common::cast::{as_float64_array, as_uint64_array};
use datafusion::common::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::{
  ColumnarValue, Expr, ReturnFieldArgs, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature,
  TypeSignature, Volatility,
};
use datafusion::prelude::col;

use crate::geometry::{
  GeometryArray, GeometryType, QuantizationOptions, QuantizedGeometry, quantize_geometry_into,
  read_geometry, to_datafusion_error,
};
use crate::output::MultiscaleEncoding;
use crate::pipeline::PipelineWarningStore;

use super::{
  GEODISPLAY_COLUMN, MultiscaleLevelSpec, POINT_M_COLUMN, POINT_X_COLUMN, POINT_Y_COLUMN,
  POINT_Z_CODE_COLUMN, POINT_Z_COLUMN, TEMP_XZ_CODE_COLUMN, XZ_CODE_COLUMN,
};

#[derive(Debug, Clone)]
/// Encodes complex geometry WKB into the complete geodisplay struct for each row.
///
/// Equality and hashing include every encoding parameter because DataFusion uses UDF
/// identity when comparing and optimizing logical expressions.
struct ComplexGeometryGeodisplayUdf {
  ty: GeometryType,
  has_z: bool,
  has_m: bool,
  levels: Vec<MultiscaleLevelSpec>,
  multiscale_encoding: MultiscaleEncoding,
  geodisplay_fields: Fields,
  dimension_warning_emitted: Arc<AtomicBool>,
  warning_store: PipelineWarningStore,
}

impl ComplexGeometryGeodisplayUdf {
  /// Construct stable output fields for the selected geometry type and multiscale levels.
  #[cfg(test)]
  fn new(ty: GeometryType, has_z: bool, has_m: bool, levels: Vec<MultiscaleLevelSpec>) -> Self {
    Self::new_with_warning_store(
      ty,
      has_z,
      has_m,
      levels,
      MultiscaleEncoding::Pbf,
      PipelineWarningStore::default(),
    )
  }

  fn new_with_warning_store(
    ty: GeometryType,
    has_z: bool,
    has_m: bool,
    levels: Vec<MultiscaleLevelSpec>,
    multiscale_encoding: MultiscaleEncoding,
    warning_store: PipelineWarningStore,
  ) -> Self {
    let mut geodisplay_fields = vec![Arc::new(Field::new(
      XZ_CODE_COLUMN,
      DataType::UInt64,
      false,
    ))];
    for encoding in &levels {
      geodisplay_fields.push(Arc::new(Field::new(
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
      geodisplay_fields: Fields::from(geodisplay_fields),
      dimension_warning_emitted: Arc::new(AtomicBool::new(false)),
      warning_store,
    }
  }

  /// Decode each non-null WKB value once and encode every configured multiscale level.
  fn encode_geodisplay(
    &self,
    geometry: &GeometryArray<'_>,
    xz_code: &UInt64Array,
  ) -> DataFusionResult<StructArray> {
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
          let geometry = read_geometry(bytes).map_err(to_datafusion_error)?;
          if geometry.ty != self.ty {
            return Err(DataFusionError::Execution(format!(
              "WKB geometry type {:?} does not match expected type {:?}",
              geometry.ty, self.ty
            )));
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
              self.warning_store.record(format!(
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
            quantize_geometry_into(&geometry, options, &mut quantized_geometry)
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

    let mut geodisplay_columns: Vec<ArrayRef> = vec![Arc::new(xz_code.clone())];
    for builder in level_builders {
      geodisplay_columns.push(builder.finish());
    }

    StructArray::try_new(self.geodisplay_fields.clone(), geodisplay_columns, None)
      .map_err(to_datafusion_error)
  }
}

impl PartialEq for ComplexGeometryGeodisplayUdf {
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

impl Eq for ComplexGeometryGeodisplayUdf {}

impl Hash for ComplexGeometryGeodisplayUdf {
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

impl ScalarUDFImpl for ComplexGeometryGeodisplayUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "geodisplay_complex_geometry"
  }

  fn signature(&self) -> &Signature {
    multiscale_signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::Struct(self.geodisplay_fields.clone()))
  }

  fn return_field_from_args(&self, _: ReturnFieldArgs) -> DataFusionResult<FieldRef> {
    Ok(Arc::new(Field::new(
      self.name(),
      DataType::Struct(self.geodisplay_fields.clone()),
      false,
    )))
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let geometry = arrays
      .first()
      .ok_or_else(|| DataFusionError::Execution("missing geometry argument".to_string()))?;
    let xz_code = as_uint64_array(arrays[1].as_ref())?;

    let geometry = GeometryArray::try_new(geometry.as_ref())?;
    let output = self.encode_geodisplay(&geometry, xz_code)?;

    Ok(ColumnarValue::Array(Arc::new(output) as ArrayRef))
  }
}

#[derive(Debug)]
/// Packs generated point index columns into the geodisplay struct.
struct PointGeometryGeodisplayUdf {
  has_z: bool,
  has_m: bool,
  signature: Signature,
}

impl PointGeometryGeodisplayUdf {
  fn new(has_z: bool, has_m: bool) -> Self {
    let mut types = vec![DataType::UInt64, DataType::Float64, DataType::Float64];
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
}

impl PartialEq for PointGeometryGeodisplayUdf {
  fn eq(&self, other: &Self) -> bool {
    self.has_z == other.has_z && self.has_m == other.has_m
  }
}

impl Eq for PointGeometryGeodisplayUdf {}

impl Hash for PointGeometryGeodisplayUdf {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.has_z.hash(state);
    self.has_m.hash(state);
  }
}

impl ScalarUDFImpl for PointGeometryGeodisplayUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "geodisplay_point_geometry"
  }

  fn signature(&self) -> &Signature {
    &self.signature
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::Struct(point_geometry_geodisplay_fields(
      self.has_z, self.has_m,
    )))
  }

  fn return_field_from_args(&self, _: ReturnFieldArgs) -> DataFusionResult<FieldRef> {
    Ok(Arc::new(Field::new(
      self.name(),
      DataType::Struct(point_geometry_geodisplay_fields(self.has_z, self.has_m)),
      true,
    )))
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let z_code = as_uint64_array(
      arrays
        .first()
        .ok_or_else(|| DataFusionError::Execution("missing Z code argument".to_string()))?
        .as_ref(),
    )?;
    let x = as_float64_array(
      arrays
        .get(1)
        .ok_or_else(|| DataFusionError::Execution("missing x argument".to_string()))?
        .as_ref(),
    )?;
    let y = as_float64_array(
      arrays
        .get(2)
        .ok_or_else(|| DataFusionError::Execution("missing y argument".to_string()))?
        .as_ref(),
    )?;
    let nulls = z_code.nulls().cloned();
    let mut columns: Vec<ArrayRef> = vec![
      Arc::new(UInt64Array::new(z_code.values().clone(), None)),
      Arc::new(Float64Array::new(x.values().clone(), None)),
      Arc::new(Float64Array::new(y.values().clone(), None)),
    ];
    let mut argument_index = 3;
    if self.has_z {
      let z = as_float64_array(arrays[argument_index].as_ref())?;
      columns.push(Arc::new(Float64Array::new(z.values().clone(), None)));
      argument_index += 1;
    }
    if self.has_m {
      let m = as_float64_array(arrays[argument_index].as_ref())?;
      columns.push(Arc::new(Float64Array::new(m.values().clone(), None)));
    }
    let output = StructArray::try_new(
      point_geometry_geodisplay_fields(self.has_z, self.has_m),
      columns,
      nulls,
    )
    .map_err(to_datafusion_error)?;
    Ok(ColumnarValue::Array(Arc::new(output)))
  }
}

fn point_geometry_geodisplay_fields(has_z: bool, has_m: bool) -> Fields {
  let mut fields = vec![
    Arc::new(Field::new(POINT_Z_CODE_COLUMN, DataType::UInt64, false)),
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

fn complex_geometry_geodisplay_udf(
  geometry_type: GeometryType,
  has_z: bool,
  has_m: bool,
  levels: Vec<MultiscaleLevelSpec>,
  multiscale_encoding: MultiscaleEncoding,
  warning_store: PipelineWarningStore,
) -> ScalarUDF {
  ScalarUDF::new_from_impl(ComplexGeometryGeodisplayUdf::new_with_warning_store(
    geometry_type,
    has_z,
    has_m,
    levels,
    multiscale_encoding,
    warning_store,
  ))
}

fn multiscale_signature() -> &'static Signature {
  static SIGNATURE: OnceLock<Signature> = OnceLock::new();
  SIGNATURE.get_or_init(|| {
    Signature::one_of(
      [
        DataType::Binary,
        DataType::LargeBinary,
        DataType::BinaryView,
      ]
      .into_iter()
      .map(|geometry_type| TypeSignature::Exact(vec![geometry_type, DataType::UInt64]))
      .collect(),
      Volatility::Immutable,
    )
  })
}

pub(crate) fn complex_geometry_geodisplay_expr(
  geometry_column: &str,
  geometry_type: GeometryType,
  has_z: bool,
  has_m: bool,
  levels: &[MultiscaleLevelSpec],
  multiscale_encoding: MultiscaleEncoding,
  warning_store: PipelineWarningStore,
) -> Expr {
  complex_geometry_geodisplay_udf(
    geometry_type,
    has_z,
    has_m,
    levels.to_vec(),
    multiscale_encoding,
    warning_store,
  )
  .call(vec![col(geometry_column), col(TEMP_XZ_CODE_COLUMN)])
  .alias(GEODISPLAY_COLUMN)
}

pub(crate) fn point_geometry_geodisplay_expr(has_z: bool, has_m: bool) -> Expr {
  let mut arguments = vec![
    datafusion::logical_expr::expr_fn::ident(POINT_Z_CODE_COLUMN),
    col(POINT_X_COLUMN),
    col(POINT_Y_COLUMN),
  ];
  if has_z {
    arguments.push(col(POINT_Z_COLUMN));
  }
  if has_m {
    arguments.push(col(POINT_M_COLUMN));
  }
  ScalarUDF::new_from_impl(PointGeometryGeodisplayUdf::new(has_z, has_m))
    .call(arguments)
    .alias(GEODISPLAY_COLUMN)
}

#[cfg(test)]
mod tests {
  use std::collections::hash_map::DefaultHasher;
  use std::hash::{Hash, Hasher};

  use arrow_array::{BinaryArray, Int64Array, ListArray};

  use crate::geometry::decode_pbf_geometry;
  use crate::optimized::multiscale::create_multiscale_level_specs;

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

  fn hash_udf(udf: &ComplexGeometryGeodisplayUdf) -> u64 {
    let mut hasher = DefaultHasher::new();
    udf.hash(&mut hasher);
    hasher.finish()
  }

  fn assert_encoding_parameter_changes_identity(mutate: impl FnOnce(&mut MultiscaleLevelSpec)) {
    let levels =
      create_multiscale_level_specs(crate::output::DEFAULT_OUTPUT_WKID, GeometryType::Polygon)
        .expect("levels");
    let original =
      ComplexGeometryGeodisplayUdf::new(GeometryType::Polygon, false, false, levels.clone());
    let mut changed_levels = levels;
    mutate(&mut changed_levels[0]);
    let changed =
      ComplexGeometryGeodisplayUdf::new(GeometryType::Polygon, false, false, changed_levels);
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
    let levels =
      create_multiscale_level_specs(crate::output::DEFAULT_OUTPUT_WKID, GeometryType::Polygon)
        .expect("levels");
    let original =
      ComplexGeometryGeodisplayUdf::new(GeometryType::Polygon, false, false, levels.clone());
    let equal =
      ComplexGeometryGeodisplayUdf::new(GeometryType::Polygon, false, false, levels.clone());
    assert_eq!(original, equal);
    assert_eq!(hash_udf(&original), hash_udf(&equal));

    let different_type =
      ComplexGeometryGeodisplayUdf::new(GeometryType::Polyline, false, false, levels.clone());
    assert_ne!(original, different_type);

    let mut reordered = levels.clone();
    reordered.swap(0, 1);
    assert_ne!(
      original,
      ComplexGeometryGeodisplayUdf::new(GeometryType::Polygon, false, false, reordered)
    );

    let mut shortened = levels;
    shortened.pop();
    assert_ne!(
      original,
      ComplexGeometryGeodisplayUdf::new(GeometryType::Polygon, false, false, shortened,)
    );
  }

  #[test]
  fn missing_m_values_are_encoded_as_zero_for_output_zm() {
    let levels =
      create_multiscale_level_specs(crate::output::DEFAULT_OUTPUT_WKID, GeometryType::Polyline)
        .expect("levels");
    let warning_store = PipelineWarningStore::default();
    let udf = ComplexGeometryGeodisplayUdf::new_with_warning_store(
      GeometryType::Polyline,
      true,
      true,
      levels,
      MultiscaleEncoding::Pbf,
      warning_store.clone(),
    );
    let wkb = multiline_z_wkb();
    let geometry = BinaryArray::from(vec![Some(wkb.as_slice())]);
    let xz_code = UInt64Array::from(vec![0]);

    let geometry = GeometryArray::try_new(&geometry).expect("geometry");
    let geodisplay = udf
      .encode_geodisplay(&geometry, &xz_code)
      .expect("geodisplay");
    let encoded = geodisplay
      .column(1)
      .as_any()
      .downcast_ref::<BinaryArray>()
      .expect("binary geometry");
    let decoded = decode_pbf_geometry(encoded.value(0)).expect("decoded geometry");

    assert_eq!(decoded.coords.len(), 8);
    assert_eq!(decoded.coords[3], 0);
    assert_eq!(decoded.coords[7], 0);
    assert_eq!(warning_store.messages().len(), 1);
  }

  #[test]
  fn missing_m_values_are_null_for_native_output_zm() {
    let levels =
      create_multiscale_level_specs(crate::output::DEFAULT_OUTPUT_WKID, GeometryType::Polyline)
        .expect("levels");
    let warning_store = PipelineWarningStore::default();
    let udf = ComplexGeometryGeodisplayUdf::new_with_warning_store(
      GeometryType::Polyline,
      true,
      true,
      levels,
      MultiscaleEncoding::QuantizedNative,
      warning_store.clone(),
    );
    let wkb = multiline_z_wkb();
    let geometry = BinaryArray::from(vec![Some(wkb.as_slice())]);
    let xz_code = UInt64Array::from(vec![0]);

    let geometry = GeometryArray::try_new(&geometry).expect("geometry");
    let geodisplay = udf
      .encode_geodisplay(&geometry, &xz_code)
      .expect("geodisplay");
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
    assert!(warning_store.messages()[0].contains("encoding missing Z/M values as null"));
  }
}

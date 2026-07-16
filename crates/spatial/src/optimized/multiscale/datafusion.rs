//! Implements non-point multiscale geodisplay encoding.

use std::any::Any;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use arrow_array::builder::BinaryBuilder;
use arrow_array::{Array, ArrayRef, Float64Array, StructArray, UInt64Array};
use arrow_schema::{DataType, Field, FieldRef, Fields};
use datafusion::common::cast::{
  as_binary_array, as_binary_view_array, as_float64_array, as_large_binary_array, as_uint64_array,
};
use datafusion::common::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::{
  ColumnarValue, Expr, ReturnFieldArgs, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature,
  TypeSignature, Volatility,
};
use datafusion::prelude::col;

use crate::geometry::{BinaryValueAccess, to_datafusion_error};
use crate::optimized::OptimizedGeometryType;
use crate::pipeline::PipelineWarningStore;

use super::{
  GEODISPLAY_COLUMN, GeometryEncodeScratch, GeometryEncoding, POINT_M_COLUMN, POINT_X_COLUMN,
  POINT_Y_COLUMN, POINT_Z_CODE_COLUMN, POINT_Z_COLUMN, TEMP_XZ_CODE_COLUMN, XZ_CODE_COLUMN,
  encode_flat_geometry_with_scratch,
  flat_geometry_payload_from_wkb as pbf_flat_geometry_payload_from_wkb,
};

#[derive(Debug, Clone)]
/// Encodes non-point WKB into the complete geodisplay struct for each row.
///
/// Equality and hashing include every encoding parameter because DataFusion uses UDF
/// identity when comparing and optimizing logical expressions.
struct NonPointGeodisplayUdf {
  geometry_type: OptimizedGeometryType,
  has_z: bool,
  has_m: bool,
  encodings: Vec<GeometryEncoding>,
  geodisplay_fields: Fields,
  dimension_warning_emitted: Arc<AtomicBool>,
  warning_store: PipelineWarningStore,
}

impl NonPointGeodisplayUdf {
  /// Construct stable output fields for the selected geometry type and LOD encodings.
  #[cfg(test)]
  fn new(
    geometry_type: OptimizedGeometryType,
    has_z: bool,
    has_m: bool,
    encodings: Vec<GeometryEncoding>,
  ) -> Self {
    Self::new_with_warning_store(
      geometry_type,
      has_z,
      has_m,
      encodings,
      PipelineWarningStore::default(),
    )
  }

  fn new_with_warning_store(
    geometry_type: OptimizedGeometryType,
    has_z: bool,
    has_m: bool,
    encodings: Vec<GeometryEncoding>,
    warning_store: PipelineWarningStore,
  ) -> Self {
    let mut geodisplay_fields = vec![Arc::new(Field::new(
      XZ_CODE_COLUMN,
      DataType::UInt64,
      false,
    ))];
    for encoding in &encodings {
      geodisplay_fields.push(Arc::new(Field::new(
        &encoding.column,
        DataType::Binary,
        true,
      )));
    }
    Self {
      geometry_type,
      has_z,
      has_m,
      encodings,
      geodisplay_fields: Fields::from(geodisplay_fields),
      dimension_warning_emitted: Arc::new(AtomicBool::new(false)),
      warning_store,
    }
  }

  /// Decode each non-null WKB value once and encode every configured multiscale level.
  fn encode_geodisplay<T: BinaryValueAccess>(
    &self,
    geometry: &T,
    xz_code: &UInt64Array,
  ) -> DataFusionResult<StructArray> {
    let mut pbf_builders = self
      .encodings
      .iter()
      .map(|_| BinaryBuilder::with_capacity(geometry.len(), geometry.len() * 16))
      .collect::<Vec<_>>();
    let mut scratch = GeometryEncodeScratch::default();

    for index in 0..geometry.len() {
      match geometry.value_opt(index) {
        Some(bytes) => {
          let mut payload = pbf_flat_geometry_payload_from_wkb(bytes, self.geometry_type)
            .map_err(to_datafusion_error)?;
          if payload.has_z != self.has_z || payload.has_m != self.has_m {
            if !self.dimension_warning_emitted.swap(true, Ordering::Relaxed) {
              self.warning_store.record(format!(
                "Warning: WKB dimensions do not match source metadata; \
                 normalizing PBF from hasZ={}, hasM={} to hasZ={}, hasM={} and encoding missing \
                 ordinates as 0",
                payload.has_z, payload.has_m, self.has_z, self.has_m
              ));
            }
            payload.has_z = self.has_z;
            payload.has_m = self.has_m;
          }
          for (builder, encoding) in pbf_builders.iter_mut().zip(&self.encodings) {
            let encoded = encode_flat_geometry_with_scratch(&payload, encoding, &mut scratch)
              .map_err(to_datafusion_error)?;
            builder.append_value(encoded);
          }
        }
        None => {
          for builder in &mut pbf_builders {
            builder.append_null();
          }
        }
      }
    }

    let mut geodisplay_columns: Vec<ArrayRef> = vec![Arc::new(xz_code.clone())];
    for mut builder in pbf_builders {
      geodisplay_columns.push(Arc::new(builder.finish()));
    }

    StructArray::try_new(self.geodisplay_fields.clone(), geodisplay_columns, None)
      .map_err(to_datafusion_error)
  }
}

impl PartialEq for NonPointGeodisplayUdf {
  fn eq(&self, other: &Self) -> bool {
    self.geometry_type == other.geometry_type
      && self.has_z == other.has_z
      && self.has_m == other.has_m
      && self.encodings.len() == other.encodings.len()
      && self
        .encodings
        .iter()
        .zip(&other.encodings)
        .all(|(left, right)| {
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

impl Eq for NonPointGeodisplayUdf {}

impl Hash for NonPointGeodisplayUdf {
  fn hash<H: Hasher>(&self, state: &mut H) {
    match self.geometry_type {
      OptimizedGeometryType::Point => 0u8,
      OptimizedGeometryType::MultiPoint => 1u8,
      OptimizedGeometryType::Polyline => 2u8,
      OptimizedGeometryType::Polygon => 3u8,
    }
    .hash(state);
    self.has_z.hash(state);
    self.has_m.hash(state);
    self.encodings.len().hash(state);
    for encoding in &self.encodings {
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

impl ScalarUDFImpl for NonPointGeodisplayUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "geodisplay_nonpoint"
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

    let output = match geometry.data_type() {
      DataType::Binary => self.encode_geodisplay(as_binary_array(geometry.as_ref())?, xz_code)?,
      DataType::LargeBinary => {
        self.encode_geodisplay(as_large_binary_array(geometry.as_ref())?, xz_code)?
      }
      DataType::BinaryView => {
        self.encode_geodisplay(as_binary_view_array(geometry.as_ref())?, xz_code)?
      }
      other => {
        return Err(DataFusionError::Execution(format!(
          "unsupported geometry data type for UDF: {other}"
        )));
      }
    };

    Ok(ColumnarValue::Array(Arc::new(output) as ArrayRef))
  }
}

#[derive(Debug)]
/// Packs generated point index columns into the geodisplay struct.
struct PointGeodisplayUdf {
  has_z: bool,
  has_m: bool,
  signature: Signature,
}

impl PointGeodisplayUdf {
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

impl PartialEq for PointGeodisplayUdf {
  fn eq(&self, other: &Self) -> bool {
    self.has_z == other.has_z && self.has_m == other.has_m
  }
}

impl Eq for PointGeodisplayUdf {}

impl Hash for PointGeodisplayUdf {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.has_z.hash(state);
    self.has_m.hash(state);
  }
}

impl ScalarUDFImpl for PointGeodisplayUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "geodisplay_point"
  }

  fn signature(&self) -> &Signature {
    &self.signature
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::Struct(point_geodisplay_fields(
      self.has_z, self.has_m,
    )))
  }

  fn return_field_from_args(&self, _: ReturnFieldArgs) -> DataFusionResult<FieldRef> {
    Ok(Arc::new(Field::new(
      self.name(),
      DataType::Struct(point_geodisplay_fields(self.has_z, self.has_m)),
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
      point_geodisplay_fields(self.has_z, self.has_m),
      columns,
      nulls,
    )
    .map_err(to_datafusion_error)?;
    Ok(ColumnarValue::Array(Arc::new(output)))
  }
}

fn point_geodisplay_fields(has_z: bool, has_m: bool) -> Fields {
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

fn non_point_geodisplay_udf(
  geometry_type: OptimizedGeometryType,
  has_z: bool,
  has_m: bool,
  encodings: Vec<GeometryEncoding>,
  warning_store: PipelineWarningStore,
) -> ScalarUDF {
  ScalarUDF::new_from_impl(NonPointGeodisplayUdf::new_with_warning_store(
    geometry_type,
    has_z,
    has_m,
    encodings,
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

pub(in crate::optimized) fn non_point_geodisplay_expr(
  geometry_column: &str,
  geometry_type: OptimizedGeometryType,
  has_z: bool,
  has_m: bool,
  encodings: &[GeometryEncoding],
  warning_store: PipelineWarningStore,
) -> Expr {
  non_point_geodisplay_udf(
    geometry_type,
    has_z,
    has_m,
    encodings.to_vec(),
    warning_store,
  )
  .call(vec![col(geometry_column), col(TEMP_XZ_CODE_COLUMN)])
  .alias(GEODISPLAY_COLUMN)
}

pub(in crate::optimized) fn point_geodisplay_expr(has_z: bool, has_m: bool) -> Expr {
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
  ScalarUDF::new_from_impl(PointGeodisplayUdf::new(has_z, has_m))
    .call(arguments)
    .alias(GEODISPLAY_COLUMN)
}

#[cfg(test)]
mod tests {
  use std::collections::hash_map::DefaultHasher;
  use std::hash::{Hash, Hasher};

  use arrow_array::BinaryArray;

  use crate::optimized::multiscale::{create_geometry_encodings, decode_pbf_geometry};

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
      for ordinate in coordinate {
        bytes.extend_from_slice(&ordinate.to_le_bytes());
      }
    }
    bytes
  }

  fn hash_udf(udf: &NonPointGeodisplayUdf) -> u64 {
    let mut hasher = DefaultHasher::new();
    udf.hash(&mut hasher);
    hasher.finish()
  }

  fn assert_encoding_parameter_changes_identity(mutate: impl FnOnce(&mut GeometryEncoding)) {
    let encodings = create_geometry_encodings(
      crate::output::DEFAULT_OUTPUT_WKID,
      OptimizedGeometryType::Polygon,
    )
    .expect("encodings");
    let original = NonPointGeodisplayUdf::new(
      OptimizedGeometryType::Polygon,
      false,
      false,
      encodings.clone(),
    );
    let mut changed_encodings = encodings;
    mutate(&mut changed_encodings[0]);
    let changed = NonPointGeodisplayUdf::new(
      OptimizedGeometryType::Polygon,
      false,
      false,
      changed_encodings,
    );
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
    let encodings = create_geometry_encodings(
      crate::output::DEFAULT_OUTPUT_WKID,
      OptimizedGeometryType::Polygon,
    )
    .expect("encodings");
    let original = NonPointGeodisplayUdf::new(
      OptimizedGeometryType::Polygon,
      false,
      false,
      encodings.clone(),
    );
    let equal = NonPointGeodisplayUdf::new(
      OptimizedGeometryType::Polygon,
      false,
      false,
      encodings.clone(),
    );
    assert_eq!(original, equal);
    assert_eq!(hash_udf(&original), hash_udf(&equal));

    let different_type = NonPointGeodisplayUdf::new(
      OptimizedGeometryType::Polyline,
      false,
      false,
      encodings.clone(),
    );
    assert_ne!(original, different_type);

    let mut reordered = encodings.clone();
    reordered.swap(0, 1);
    assert_ne!(
      original,
      NonPointGeodisplayUdf::new(OptimizedGeometryType::Polygon, false, false, reordered)
    );

    let mut shortened = encodings;
    shortened.pop();
    assert_ne!(
      original,
      NonPointGeodisplayUdf::new(OptimizedGeometryType::Polygon, false, false, shortened)
    );
  }

  #[test]
  fn missing_m_ordinates_are_encoded_as_zero_for_zm_output() {
    let encodings = create_geometry_encodings(
      crate::output::DEFAULT_OUTPUT_WKID,
      OptimizedGeometryType::Polyline,
    )
    .expect("encodings");
    let warning_store = PipelineWarningStore::default();
    let udf = NonPointGeodisplayUdf::new_with_warning_store(
      OptimizedGeometryType::Polyline,
      true,
      true,
      encodings,
      warning_store.clone(),
    );
    let wkb = multiline_z_wkb();
    let geometry = BinaryArray::from(vec![Some(wkb.as_slice())]);
    let xz_code = UInt64Array::from(vec![0]);

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
}

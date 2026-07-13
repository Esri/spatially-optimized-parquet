//! Implements non-point multiscale geodisplay encoding.

use std::any::Any;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, OnceLock};

use arrow_array::builder::BinaryBuilder;
use arrow_array::{Array, ArrayRef, Float64Array, StructArray, UInt64Array};
use arrow_schema::{DataType, Field, Fields};
use datafusion::common::cast::{
  as_binary_array, as_binary_view_array, as_float64_array, as_large_binary_array, as_uint64_array,
};
use datafusion::common::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::{
  ColumnarValue, Expr, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature, TypeSignature,
  Volatility,
};
use datafusion::prelude::col;

use crate::optimized::OptimizedGeometryType;
use crate::output::geometry::{BinaryValueAccess, to_datafusion_error};

use super::{
  BOUNDS_COLUMN, DISPLAY_COLUMN, GeometryEncodeScratch, GeometryEncoding, TEMP_XMAX_COLUMN,
  TEMP_XMIN_COLUMN, TEMP_XZ_CODE_COLUMN, TEMP_YMAX_COLUMN, TEMP_YMIN_COLUMN, XZ_CODE_COLUMN,
  encode_flat_geometry_with_scratch,
  flat_geometry_payload_from_wkb as pbf_flat_geometry_payload_from_wkb,
};

#[derive(Debug, Clone)]
/// Encodes non-point WKB into the complete geodisplay struct for each row.
///
/// Equality and hashing include every encoding parameter because DataFusion uses UDF
/// identity when comparing and optimizing logical expressions.
pub(crate) struct NonPointGeodisplayUdf {
  geometry_type: OptimizedGeometryType,
  encodings: Vec<GeometryEncoding>,
  display_fields: Fields,
  bounds_fields: Fields,
}

impl NonPointGeodisplayUdf {
  /// Build stable output fields for the selected geometry type and LOD encodings.
  fn new(geometry_type: OptimizedGeometryType, encodings: Vec<GeometryEncoding>) -> Self {
    let bounds_fields = Fields::from(vec![
      Arc::new(Field::new("xmin", DataType::Float64, true)),
      Arc::new(Field::new("ymin", DataType::Float64, true)),
      Arc::new(Field::new("xmax", DataType::Float64, true)),
      Arc::new(Field::new("ymax", DataType::Float64, true)),
    ]);
    let mut display_fields = vec![
      Arc::new(Field::new(XZ_CODE_COLUMN, DataType::UInt64, false)),
      Arc::new(Field::new(
        BOUNDS_COLUMN,
        DataType::Struct(bounds_fields.clone()),
        true,
      )),
    ];
    for encoding in &encodings {
      display_fields.push(Arc::new(Field::new(
        &encoding.column,
        DataType::Binary,
        true,
      )));
    }
    Self {
      geometry_type,
      encodings,
      display_fields: Fields::from(display_fields),
      bounds_fields,
    }
  }

  /// Decode each non-null WKB value once and encode every configured display level.
  fn build_display<T: BinaryValueAccess>(
    &self,
    geometry: &T,
    xz_code: &UInt64Array,
    xmin: &Float64Array,
    ymin: &Float64Array,
    xmax: &Float64Array,
    ymax: &Float64Array,
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
          let payload = pbf_flat_geometry_payload_from_wkb(bytes, self.geometry_type)
            .map_err(to_datafusion_error)?;
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

    let bounds = StructArray::try_new(
      self.bounds_fields.clone(),
      vec![
        Arc::new(xmin.clone()),
        Arc::new(ymin.clone()),
        Arc::new(xmax.clone()),
        Arc::new(ymax.clone()),
      ],
      xmin.nulls().cloned(),
    )
    .map_err(to_datafusion_error)?;

    let mut display_columns: Vec<ArrayRef> = vec![Arc::new(xz_code.clone()), Arc::new(bounds)];
    for mut builder in pbf_builders {
      display_columns.push(Arc::new(builder.finish()));
    }

    StructArray::try_new(self.display_fields.clone(), display_columns, None)
      .map_err(to_datafusion_error)
  }
}

impl PartialEq for NonPointGeodisplayUdf {
  fn eq(&self, other: &Self) -> bool {
    self.geometry_type == other.geometry_type
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
    "display_nonpoint_geodisplay"
  }

  fn signature(&self) -> &Signature {
    multiscale_signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::Struct(self.display_fields.clone()))
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let geometry = arrays
      .first()
      .ok_or_else(|| DataFusionError::Execution("missing geometry argument".to_string()))?;
    let xz_code = as_uint64_array(arrays[1].as_ref())?;
    let xmin = as_float64_array(arrays[2].as_ref())?;
    let ymin = as_float64_array(arrays[3].as_ref())?;
    let xmax = as_float64_array(arrays[4].as_ref())?;
    let ymax = as_float64_array(arrays[5].as_ref())?;

    let output = match geometry.data_type() {
      DataType::Binary => self.build_display(
        as_binary_array(geometry.as_ref())?,
        xz_code,
        xmin,
        ymin,
        xmax,
        ymax,
      )?,
      DataType::LargeBinary => self.build_display(
        as_large_binary_array(geometry.as_ref())?,
        xz_code,
        xmin,
        ymin,
        xmax,
        ymax,
      )?,
      DataType::BinaryView => self.build_display(
        as_binary_view_array(geometry.as_ref())?,
        xz_code,
        xmin,
        ymin,
        xmax,
        ymax,
      )?,
      other => {
        return Err(DataFusionError::Execution(format!(
          "unsupported geometry data type for UDF: {other}"
        )));
      }
    };

    Ok(ColumnarValue::Array(Arc::new(output) as ArrayRef))
  }
}
pub(crate) fn non_point_geodisplay_udf(
  geometry_type: OptimizedGeometryType,
  encodings: Vec<GeometryEncoding>,
) -> ScalarUDF {
  ScalarUDF::new_from_impl(NonPointGeodisplayUdf::new(geometry_type, encodings))
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
      .map(|geometry_type| {
        TypeSignature::Exact(vec![
          geometry_type,
          DataType::UInt64,
          DataType::Float64,
          DataType::Float64,
          DataType::Float64,
          DataType::Float64,
        ])
      })
      .collect(),
      Volatility::Immutable,
    )
  })
}

pub(crate) fn non_point_geodisplay_expr(
  geometry_column: &str,
  geometry_type: OptimizedGeometryType,
  encodings: &[GeometryEncoding],
) -> Expr {
  non_point_geodisplay_udf(geometry_type, encodings.to_vec())
    .call(vec![
      col(geometry_column),
      col(TEMP_XZ_CODE_COLUMN),
      col(TEMP_XMIN_COLUMN),
      col(TEMP_YMIN_COLUMN),
      col(TEMP_XMAX_COLUMN),
      col(TEMP_YMAX_COLUMN),
    ])
    .alias(DISPLAY_COLUMN)
}

#[cfg(test)]
mod tests {
  use std::collections::hash_map::DefaultHasher;
  use std::hash::{Hash, Hasher};

  use crate::optimized::multiscale::create_geometry_encodings;

  use super::*;

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
    let original = NonPointGeodisplayUdf::new(OptimizedGeometryType::Polygon, encodings.clone());
    let mut changed_encodings = encodings;
    mutate(&mut changed_encodings[0]);
    let changed = NonPointGeodisplayUdf::new(OptimizedGeometryType::Polygon, changed_encodings);
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
    let original = NonPointGeodisplayUdf::new(OptimizedGeometryType::Polygon, encodings.clone());
    let equal = NonPointGeodisplayUdf::new(OptimizedGeometryType::Polygon, encodings.clone());
    assert_eq!(original, equal);
    assert_eq!(hash_udf(&original), hash_udf(&equal));

    let different_type =
      NonPointGeodisplayUdf::new(OptimizedGeometryType::Polyline, encodings.clone());
    assert_ne!(original, different_type);

    let mut reordered = encodings.clone();
    reordered.swap(0, 1);
    assert_ne!(
      original,
      NonPointGeodisplayUdf::new(OptimizedGeometryType::Polygon, reordered)
    );

    let mut shortened = encodings;
    shortened.pop();
    assert_ne!(
      original,
      NonPointGeodisplayUdf::new(OptimizedGeometryType::Polygon, shortened)
    );
  }
}

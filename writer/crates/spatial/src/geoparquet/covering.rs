//! Creates GeoParquet covering bbox structs through typed DataFusion expressions.

use std::any::Any;
use std::sync::{Arc, OnceLock};

use anyhow::{Result, bail};
use arrow_array::{ArrayRef, Float64Array, StructArray};
use arrow_schema::{DataType, Field, Fields, Schema};
use datafusion::common::cast::as_float64_array;
use datafusion::common::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::{
  ColumnarValue, Expr, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature, TypeSignature,
  Volatility,
};
use datafusion::prelude::col;

use crate::geometry::to_datafusion_error;
use crate::optimized::multiscale::COVERING_BBOX_COLUMN;

/// Reject covering output that would overwrite an existing source column.
pub(crate) fn validate_covering_configuration(covering: bool, schema: &Schema) -> Result<()> {
  if covering && schema.field_with_name(COVERING_BBOX_COLUMN).is_ok() {
    bail!("--covering would overwrite existing input column '{COVERING_BBOX_COLUMN}'");
  }
  Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FeatureBboxUdf;

impl ScalarUDFImpl for FeatureBboxUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "geoparquet_feature_bbox"
  }

  fn signature(&self) -> &Signature {
    covering_signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::Struct(bounds_fields()))
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let geometry = arrays
      .first()
      .ok_or_else(|| DataFusionError::Execution("missing geometry argument".to_string()))?;
    let xmin = float_argument(&arrays, 1, "xmin")?;
    let ymin = float_argument(&arrays, 2, "ymin")?;
    let xmax = float_argument(&arrays, 3, "xmax")?;
    let ymax = float_argument(&arrays, 4, "ymax")?;
    let output = StructArray::try_new(
      bounds_fields(),
      vec![
        Arc::new(xmin.clone()),
        Arc::new(ymin.clone()),
        Arc::new(xmax.clone()),
        Arc::new(ymax.clone()),
      ],
      geometry.nulls().cloned(),
    )
    .map_err(to_datafusion_error)?;
    Ok(ColumnarValue::Array(Arc::new(output) as ArrayRef))
  }
}

pub(crate) fn feature_bbox_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(FeatureBboxUdf)
}

pub(crate) fn feature_bbox_expr(
  geometry_column: &str,
  xmin_column: &str,
  ymin_column: &str,
  xmax_column: &str,
  ymax_column: &str,
) -> Expr {
  feature_bbox_udf()
    .call(vec![
      col(geometry_column),
      col(xmin_column),
      col(ymin_column),
      col(xmax_column),
      col(ymax_column),
    ])
    .alias(COVERING_BBOX_COLUMN)
}

fn float_argument<'a>(
  arrays: &'a [ArrayRef],
  index: usize,
  name: &str,
) -> DataFusionResult<&'a Float64Array> {
  as_float64_array(
    arrays
      .get(index)
      .ok_or_else(|| DataFusionError::Execution(format!("missing {name} argument")))?
      .as_ref(),
  )
}

fn bounds_fields() -> Fields {
  static FIELDS: OnceLock<Fields> = OnceLock::new();
  FIELDS
    .get_or_init(|| {
      Fields::from(vec![
        Arc::new(Field::new("xmin", DataType::Float64, true)),
        Arc::new(Field::new("ymin", DataType::Float64, true)),
        Arc::new(Field::new("xmax", DataType::Float64, true)),
        Arc::new(Field::new("ymax", DataType::Float64, true)),
      ])
    })
    .clone()
}

fn covering_signature() -> &'static Signature {
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

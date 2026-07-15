//! Creates GeoParquet covering bbox structs through typed DataFusion expressions.

use std::any::Any;
use std::sync::{Arc, OnceLock};

use arrow_array::{ArrayRef, Float64Array, StructArray};
use arrow_schema::{DataType, Field, Fields};
use datafusion::common::cast::as_float64_array;
use datafusion::common::{DataFusionError, Result as DataFusionResult};
use datafusion::functions::core::expr_ext::FieldAccessor;
use datafusion::logical_expr::{
  ColumnarValue, Expr, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature, TypeSignature,
  Volatility,
};
use datafusion::prelude::col;

use crate::geometry::GeometryCategory;
use crate::geometry::to_datafusion_error;
use crate::optimized::{COVERING_BBOX_COLUMN, bounds_expr, point_expr};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FeatureBboxUdf;

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
    let nulls = geometry.nulls().cloned();
    let output = StructArray::try_new(
      bounds_fields(),
      vec![
        Arc::new(Float64Array::new(xmin.values().clone(), nulls.clone())),
        Arc::new(Float64Array::new(ymin.values().clone(), nulls.clone())),
        Arc::new(Float64Array::new(xmax.values().clone(), nulls.clone())),
        Arc::new(Float64Array::new(ymax.values().clone(), nulls.clone())),
      ],
      nulls,
    )
    .map_err(to_datafusion_error)?;
    Ok(ColumnarValue::Array(Arc::new(output) as ArrayRef))
  }
}

fn feature_bbox_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(FeatureBboxUdf)
}

pub(crate) fn geometry_bbox_expr(
  geometry_column: &str,
  geometry_category: GeometryCategory,
) -> Expr {
  let geometry = col(geometry_column);
  match geometry_category {
    GeometryCategory::Point => {
      let coordinates = point_expr(geometry_column);
      let x = coordinates.clone().field("x");
      let y = coordinates.field("y");
      feature_bbox_udf()
        .call(vec![geometry, x.clone(), y.clone(), x, y])
        .alias(COVERING_BBOX_COLUMN)
    }
    GeometryCategory::NonPoint => {
      let bounds = bounds_expr(geometry_column);
      feature_bbox_udf()
        .call(vec![
          geometry,
          bounds.clone().field("xmin"),
          bounds.clone().field("ymin"),
          bounds.clone().field("xmax"),
          bounds.field("ymax"),
        ])
        .alias(COVERING_BBOX_COLUMN)
    }
  }
}

pub(crate) fn bbox_field_expr(field: &str) -> Expr {
  col(COVERING_BBOX_COLUMN).field(field)
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

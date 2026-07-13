//! Implements point extraction and Z cluster-key UDFs.

#![allow(dead_code)]

use std::any::Any;
use std::sync::{Arc, OnceLock};

use arrow_array::{Array, ArrayRef, Float64Array, StructArray, UInt64Array};
use arrow_schema::{DataType, Field, Fields};
use datafusion::common::cast::{
  as_binary_array, as_binary_view_array, as_float64_array, as_large_binary_array,
};
use datafusion::common::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::{
  ColumnarValue, Expr, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature, TypeSignature,
  Volatility,
};
use datafusion::prelude::{col, lit};

use crate::geometry::{
  BinaryValueAccess, Extent2D, geometry_signature, map_geometry_to_u64, to_datafusion_error,
};
use crate::optimized::multiscale::{POINT_Z_CODE_COLUMN, point_xy_from_wkb};

use super::algorithm::{DEFAULT_COORDINATE_PRECISION, point_z_code};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PointUdf;

impl ScalarUDFImpl for PointUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "clustering_point"
  }

  fn signature(&self) -> &Signature {
    geometry_signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::Struct(point_fields()))
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let geometry = arrays
      .first()
      .ok_or_else(|| DataFusionError::Execution("missing geometry argument".to_string()))?;
    let output = match geometry.data_type() {
      DataType::Binary => point_coords_struct(as_binary_array(geometry.as_ref())?)?,
      DataType::LargeBinary => point_coords_struct(as_large_binary_array(geometry.as_ref())?)?,
      DataType::BinaryView => point_coords_struct(as_binary_view_array(geometry.as_ref())?)?,
      other => {
        return Err(DataFusionError::Execution(format!(
          "unsupported geometry data type for UDF: {other}"
        )));
      }
    };
    Ok(ColumnarValue::Array(Arc::new(output) as ArrayRef))
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ZGeometryClusterKeyUdf;

impl ScalarUDFImpl for ZGeometryClusterKeyUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "clustering_point_zcode"
  }

  fn signature(&self) -> &Signature {
    geometry_cluster_signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::UInt64)
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let geometry = arrays
      .first()
      .ok_or_else(|| DataFusionError::Execution("missing geometry argument".to_string()))?;
    let full_extent = extent_from_args(&arrays, 1)?;
    let output = map_geometry_to_u64(geometry, |bytes| match bytes {
      Some(bytes) => {
        let (x, y) = point_xy_from_wkb(bytes).map_err(to_datafusion_error)?;
        Ok(point_z_code(full_extent, x, y, DEFAULT_COORDINATE_PRECISION).value())
      }
      None => Ok(0),
    })?;
    Ok(ColumnarValue::Array(Arc::new(output) as ArrayRef))
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ZPointClusterKeyUdf;

impl ScalarUDFImpl for ZPointClusterKeyUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "clustering_point_zcode_from_xy"
  }

  fn signature(&self) -> &Signature {
    z_cluster_signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::UInt64)
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let x = as_float64_array(arrays[0].as_ref())?;
    let y = as_float64_array(arrays[1].as_ref())?;
    let full_extent = extent_from_args(&arrays, 2)?;
    let mut values = Vec::with_capacity(x.len());
    for index in 0..x.len() {
      values.push(if x.is_null(index) || y.is_null(index) {
        0
      } else {
        point_z_code(
          full_extent,
          x.value(index),
          y.value(index),
          DEFAULT_COORDINATE_PRECISION,
        )
        .value()
      });
    }
    Ok(ColumnarValue::Array(
      Arc::new(UInt64Array::from(values)) as ArrayRef
    ))
  }
}

pub(crate) fn point_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(PointUdf)
}

pub(crate) fn point_zcode_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(ZGeometryClusterKeyUdf)
}

pub(crate) fn point_zcode_from_xy_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(ZPointClusterKeyUdf)
}

fn point_coords_struct<T>(array: &T) -> DataFusionResult<StructArray>
where
  T: BinaryValueAccess,
{
  let mut xs = Vec::with_capacity(array.len());
  let mut ys = Vec::with_capacity(array.len());
  for index in 0..array.len() {
    match array.value_opt(index) {
      Some(bytes) => {
        let (x, y) = point_xy_from_wkb(bytes).map_err(to_datafusion_error)?;
        xs.push(Some(x));
        ys.push(Some(y));
      }
      None => {
        xs.push(None);
        ys.push(None);
      }
    }
  }
  let x_array = Float64Array::from(xs);
  let y_array = Float64Array::from(ys);
  StructArray::try_new(
    point_fields(),
    vec![Arc::new(x_array.clone()), Arc::new(y_array)],
    x_array.nulls().cloned(),
  )
  .map_err(to_datafusion_error)
}

fn point_fields() -> Fields {
  static FIELDS: OnceLock<Fields> = OnceLock::new();
  FIELDS
    .get_or_init(|| {
      Fields::from(vec![
        Arc::new(Field::new("x", DataType::Float64, true)),
        Arc::new(Field::new("y", DataType::Float64, true)),
      ])
    })
    .clone()
}

fn geometry_cluster_signature() -> &'static Signature {
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

fn z_cluster_signature() -> &'static Signature {
  static SIGNATURE: OnceLock<Signature> = OnceLock::new();
  SIGNATURE.get_or_init(|| Signature::exact(vec![DataType::Float64; 6], Volatility::Immutable))
}

pub(crate) fn point_zcode_expr(geometry_column: &str, full_extent: Extent2D) -> Expr {
  point_zcode_udf()
    .call(vec![
      col(geometry_column),
      lit(full_extent.xmin),
      lit(full_extent.ymin),
      lit(full_extent.xmax),
      lit(full_extent.ymax),
    ])
    .alias(POINT_Z_CODE_COLUMN)
}

pub(crate) fn point_expr(geometry_column: &str) -> Expr {
  point_udf().call(vec![col(geometry_column)])
}

pub(crate) fn point_zcode_from_xy_expr(
  x_column: &str,
  y_column: &str,
  full_extent: Extent2D,
) -> Expr {
  point_zcode_from_xy_udf()
    .call(vec![
      col(x_column),
      col(y_column),
      lit(full_extent.xmin),
      lit(full_extent.ymin),
      lit(full_extent.xmax),
      lit(full_extent.ymax),
    ])
    .alias(POINT_Z_CODE_COLUMN)
}

fn extent_from_args(arrays: &[ArrayRef], start: usize) -> DataFusionResult<Extent2D> {
  Ok(Extent2D {
    xmin: first_f64(&arrays[start])?,
    ymin: first_f64(&arrays[start + 1])?,
    xmax: first_f64(&arrays[start + 2])?,
    ymax: first_f64(&arrays[start + 3])?,
  })
}

fn first_f64(array: &ArrayRef) -> DataFusionResult<f64> {
  let array = as_float64_array(array.as_ref())?;
  if array.is_empty() || array.is_null(0) {
    return Err(DataFusionError::Execution(
      "missing full extent argument for Z clustering".to_string(),
    ));
  }
  Ok(array.value(0))
}

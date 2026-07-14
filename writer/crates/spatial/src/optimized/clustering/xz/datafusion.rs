//! Implements bounds extraction and XZ cluster-key UDFs.

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
use crate::optimized::multiscale::TEMP_XZ_CODE_COLUMN;
use crate::optimized::multiscale::geometry_extent_from_wkb;

use super::algorithm::{DEFAULT_XZ_MAX_LEVEL, extent_xz_code};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct BoundsUdf;

impl ScalarUDFImpl for BoundsUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "clustering_bounds"
  }

  fn signature(&self) -> &Signature {
    geometry_signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::Struct(bounds_fields()))
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let geometry = arrays
      .first()
      .ok_or_else(|| DataFusionError::Execution("missing geometry argument".to_string()))?;
    let output = match geometry.data_type() {
      DataType::Binary => bounds_struct(as_binary_array(geometry.as_ref())?)?,
      DataType::LargeBinary => bounds_struct(as_large_binary_array(geometry.as_ref())?)?,
      DataType::BinaryView => bounds_struct(as_binary_view_array(geometry.as_ref())?)?,
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
struct XZGeometryClusterKeyUdf;

impl ScalarUDFImpl for XZGeometryClusterKeyUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "clustering_nonpoint_xzcode"
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
      Some(bytes) => Ok(
        extent_xz_code(
          full_extent,
          geometry_extent_from_wkb(bytes).map_err(to_datafusion_error)?,
          DEFAULT_XZ_MAX_LEVEL,
        )
        .value(),
      ),
      None => Ok(0),
    })?;
    Ok(ColumnarValue::Array(Arc::new(output) as ArrayRef))
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct XZBoundsClusterKeyUdf;

impl ScalarUDFImpl for XZBoundsClusterKeyUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "clustering_nonpoint_xzcode_from_bounds"
  }

  fn signature(&self) -> &Signature {
    xz_cluster_signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::UInt64)
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let xmin = as_float64_array(arrays[0].as_ref())?;
    let ymin = as_float64_array(arrays[1].as_ref())?;
    let xmax = as_float64_array(arrays[2].as_ref())?;
    let ymax = as_float64_array(arrays[3].as_ref())?;
    let full_extent = extent_from_args(&arrays, 4)?;
    let mut values = Vec::with_capacity(xmin.len());
    for index in 0..xmin.len() {
      values.push(
        if xmin.is_null(index) || ymin.is_null(index) || xmax.is_null(index) || ymax.is_null(index)
        {
          0
        } else {
          extent_xz_code(
            full_extent,
            Extent2D {
              xmin: xmin.value(index),
              ymin: ymin.value(index),
              xmax: xmax.value(index),
              ymax: ymax.value(index),
            },
            DEFAULT_XZ_MAX_LEVEL,
          )
          .value()
        },
      );
    }
    Ok(ColumnarValue::Array(
      Arc::new(UInt64Array::from(values)) as ArrayRef
    ))
  }
}

fn bounds_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(BoundsUdf)
}

fn non_point_xzcode_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(XZGeometryClusterKeyUdf)
}

fn non_point_xzcode_from_bounds_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(XZBoundsClusterKeyUdf)
}

fn bounds_struct<T>(array: &T) -> DataFusionResult<StructArray>
where
  T: BinaryValueAccess,
{
  let mut xmin_values = Vec::with_capacity(array.len());
  let mut ymin_values = Vec::with_capacity(array.len());
  let mut xmax_values = Vec::with_capacity(array.len());
  let mut ymax_values = Vec::with_capacity(array.len());
  for index in 0..array.len() {
    match array.value_opt(index) {
      Some(bytes) => {
        let extent = geometry_extent_from_wkb(bytes).map_err(to_datafusion_error)?;
        xmin_values.push(Some(extent.xmin));
        ymin_values.push(Some(extent.ymin));
        xmax_values.push(Some(extent.xmax));
        ymax_values.push(Some(extent.ymax));
      }
      None => {
        xmin_values.push(None);
        ymin_values.push(None);
        xmax_values.push(None);
        ymax_values.push(None);
      }
    }
  }
  let xmin_array = Float64Array::from(xmin_values);
  let ymin_array = Float64Array::from(ymin_values);
  let xmax_array = Float64Array::from(xmax_values);
  let ymax_array = Float64Array::from(ymax_values);
  StructArray::try_new(
    bounds_fields(),
    vec![
      Arc::new(xmin_array.clone()),
      Arc::new(ymin_array),
      Arc::new(xmax_array),
      Arc::new(ymax_array),
    ],
    xmin_array.nulls().cloned(),
  )
  .map_err(to_datafusion_error)
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

fn xz_cluster_signature() -> &'static Signature {
  static SIGNATURE: OnceLock<Signature> = OnceLock::new();
  SIGNATURE.get_or_init(|| Signature::exact(vec![DataType::Float64; 8], Volatility::Immutable))
}

pub(crate) fn bounds_expr(geometry_column: &str) -> Expr {
  bounds_udf().call(vec![col(geometry_column)])
}

pub(in crate::optimized) fn non_point_xzcode_from_bounds_expr(
  xmin_column: &str,
  ymin_column: &str,
  xmax_column: &str,
  ymax_column: &str,
  full_extent: Extent2D,
) -> Expr {
  non_point_xzcode_from_bounds_udf()
    .call(vec![
      col(xmin_column),
      col(ymin_column),
      col(xmax_column),
      col(ymax_column),
      lit(full_extent.xmin),
      lit(full_extent.ymin),
      lit(full_extent.xmax),
      lit(full_extent.ymax),
    ])
    .alias(TEMP_XZ_CODE_COLUMN)
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
      "missing full extent argument for XZ clustering".to_string(),
    ));
  }
  Ok(array.value(0))
}

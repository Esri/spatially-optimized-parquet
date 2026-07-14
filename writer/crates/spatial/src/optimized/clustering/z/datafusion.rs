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
  ColumnarValue, Expr, ReturnFieldArgs, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature,
  TypeSignature, Volatility,
};
use datafusion::prelude::{col, lit};

use crate::geometry::{
  BinaryValueAccess, Extent2D, geometry_signature, map_geometry_to_u64, to_datafusion_error,
};
use crate::optimized::multiscale::{POINT_Z_CODE_COLUMN, point_xy_from_wkb};

use super::algorithm::{DEFAULT_COORDINATE_PRECISION, point_z_code};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PointUdf;

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

  fn return_field_from_args(&self, _: ReturnFieldArgs) -> DataFusionResult<Arc<Field>> {
    Ok(Arc::new(Field::new(
      self.name(),
      DataType::Struct(point_fields()),
      false,
    )))
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
struct ZGeometryClusterKeyUdf;

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

  fn return_field_from_args(&self, _: ReturnFieldArgs) -> DataFusionResult<Arc<Field>> {
    Ok(Arc::new(Field::new(self.name(), DataType::UInt64, false)))
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
struct ZPointClusterKeyUdf;

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

  fn return_field_from_args(&self, _: ReturnFieldArgs) -> DataFusionResult<Arc<Field>> {
    Ok(Arc::new(Field::new(self.name(), DataType::UInt64, false)))
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

fn point_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(PointUdf)
}

fn point_zcode_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(ZGeometryClusterKeyUdf)
}

fn point_zcode_from_xy_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(ZPointClusterKeyUdf)
}

fn point_coords_struct<T>(array: &T) -> DataFusionResult<StructArray>
where
  T: BinaryValueAccess,
{
  let mut xs = Vec::with_capacity(array.len());
  let mut ys = Vec::with_capacity(array.len());
  for index in 0..array.len() {
    let (x, y) = array
      .value_opt(index)
      .and_then(|bytes| point_xy_from_wkb(bytes).ok())
      .unwrap_or((f64::NAN, f64::NAN));
    xs.push(x);
    ys.push(y);
  }
  let x_array = Float64Array::from(xs);
  let y_array = Float64Array::from(ys);
  StructArray::try_new(
    point_fields(),
    vec![Arc::new(x_array), Arc::new(y_array)],
    None,
  )
  .map_err(to_datafusion_error)
}

fn point_fields() -> Fields {
  static FIELDS: OnceLock<Fields> = OnceLock::new();
  FIELDS
    .get_or_init(|| {
      Fields::from(vec![
        Arc::new(Field::new("x", DataType::Float64, false)),
        Arc::new(Field::new("y", DataType::Float64, false)),
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

pub(crate) fn point_expr(geometry_column: &str) -> Expr {
  point_udf().call(vec![col(geometry_column)])
}

pub(in crate::optimized) fn point_zcode_from_xy_expr(
  x: Expr,
  y: Expr,
  full_extent: Extent2D,
) -> Expr {
  point_zcode_from_xy_udf()
    .call(vec![
      x,
      y,
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

#[cfg(test)]
mod tests {
  use arrow_array::BinaryArray;
  use geo_types::{Geometry, Point};

  use super::*;

  #[test]
  fn point_coordinates_are_required_and_use_nan_for_missing_or_invalid_geometry() {
    let mut point_wkb = Vec::new();
    wkb::writer::write_geometry(
      &mut point_wkb,
      &Geometry::Point(Point::new(1.0, 2.0)),
      &Default::default(),
    )
    .unwrap();
    let invalid_wkb = [0_u8, 1, 2];
    let input = BinaryArray::from(vec![
      Some(point_wkb.as_slice()),
      None,
      Some(invalid_wkb.as_slice()),
    ]);

    let coordinates = point_coords_struct(&input).unwrap();
    let x = coordinates
      .column_by_name("x")
      .unwrap()
      .as_any()
      .downcast_ref::<Float64Array>()
      .unwrap();
    let y = coordinates
      .column_by_name("y")
      .unwrap()
      .as_any()
      .downcast_ref::<Float64Array>()
      .unwrap();

    assert!(!coordinates.fields()[0].is_nullable());
    assert!(!coordinates.fields()[1].is_nullable());
    assert_eq!(coordinates.null_count(), 0);
    assert_eq!(x.null_count(), 0);
    assert_eq!(y.null_count(), 0);
    assert_eq!(x.value(0), 1.0);
    assert_eq!(y.value(0), 2.0);
    assert!(x.value(1).is_nan());
    assert!(y.value(1).is_nan());
    assert!(x.value(2).is_nan());
    assert!(y.value(2).is_nan());
  }
}

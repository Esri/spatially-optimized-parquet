//! Implements typed DataFusion expressions for geometry reprojection.

use std::any::Any;
use std::sync::{Arc, OnceLock};

use arrow_array::{Array, ArrayRef, Float64Array, StructArray};
use arrow_schema::{DataType, Field, Fields};
use datafusion::common::cast::{as_binary_array, as_binary_view_array, as_large_binary_array};
use datafusion::common::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::{
  ColumnarValue, Expr, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature,
};
use datafusion::prelude::col;

use crate::geometry::{
  BinaryValueAccess, GeometryCategory, geometry_signature, map_geometry_to_binary,
  to_datafusion_error,
};

use super::{CoordinateTransformSpec, PreparedTransform};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TransformedPointCoordsUdf {
  transform: CoordinateTransformSpec,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TransformedBoundsUdf {
  transform: CoordinateTransformSpec,
  geometry_category: GeometryCategory,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ReprojectGeometryUdf {
  transform: CoordinateTransformSpec,
}

impl ScalarUDFImpl for TransformedPointCoordsUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "reprojection_transformed_point_coords"
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
    let prepared = self.transform.prepare().map_err(to_datafusion_error)?;
    let output = match geometry.data_type() {
      DataType::Binary => {
        transformed_point_coords_struct(as_binary_array(geometry.as_ref())?, &prepared)?
      }
      DataType::LargeBinary => {
        transformed_point_coords_struct(as_large_binary_array(geometry.as_ref())?, &prepared)?
      }
      DataType::BinaryView => {
        transformed_point_coords_struct(as_binary_view_array(geometry.as_ref())?, &prepared)?
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

impl ScalarUDFImpl for TransformedBoundsUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "reprojection_transformed_bounds"
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
    let prepared = self.transform.prepare().map_err(to_datafusion_error)?;
    let output = match geometry.data_type() {
      DataType::Binary => transformed_bounds_struct(
        as_binary_array(geometry.as_ref())?,
        &prepared,
        self.geometry_category,
      )?,
      DataType::LargeBinary => transformed_bounds_struct(
        as_large_binary_array(geometry.as_ref())?,
        &prepared,
        self.geometry_category,
      )?,
      DataType::BinaryView => transformed_bounds_struct(
        as_binary_view_array(geometry.as_ref())?,
        &prepared,
        self.geometry_category,
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

impl ScalarUDFImpl for ReprojectGeometryUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "reprojection_geometry"
  }

  fn signature(&self) -> &Signature {
    geometry_signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::Binary)
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let geometry = arrays
      .first()
      .ok_or_else(|| DataFusionError::Execution("missing geometry argument".to_string()))?;
    let prepared = self.transform.prepare().map_err(to_datafusion_error)?;
    let output = map_geometry_to_binary(geometry, |bytes| match bytes {
      Some(bytes) => prepared
        .reproject_wkb(bytes)
        .map(Some)
        .map_err(to_datafusion_error),
      None => Ok(None),
    })?;
    Ok(ColumnarValue::Array(output))
  }
}

fn transformed_point_coords_udf(transform: CoordinateTransformSpec) -> ScalarUDF {
  ScalarUDF::new_from_impl(TransformedPointCoordsUdf { transform })
}

fn transformed_bounds_udf(
  transform: CoordinateTransformSpec,
  geometry_category: GeometryCategory,
) -> ScalarUDF {
  ScalarUDF::new_from_impl(TransformedBoundsUdf {
    transform,
    geometry_category,
  })
}

fn reproject_geometry_udf(transform: CoordinateTransformSpec) -> ScalarUDF {
  ScalarUDF::new_from_impl(ReprojectGeometryUdf { transform })
}

pub(crate) fn transformed_point_coords_expr(
  geometry_column: &str,
  transform: &CoordinateTransformSpec,
) -> Expr {
  transformed_point_coords_udf(transform.clone()).call(vec![col(geometry_column)])
}

pub(crate) fn transformed_bounds_expr(
  geometry_column: &str,
  geometry_category: GeometryCategory,
  transform: &CoordinateTransformSpec,
) -> Expr {
  transformed_bounds_udf(transform.clone(), geometry_category).call(vec![col(geometry_column)])
}

pub(crate) fn reproject_geometry_expr(
  geometry_column: &str,
  transform: &CoordinateTransformSpec,
) -> Expr {
  reproject_geometry_udf(transform.clone()).call(vec![col(geometry_column)])
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

fn transformed_point_coords_struct<T>(
  array: &T,
  transform: &PreparedTransform,
) -> DataFusionResult<StructArray>
where
  T: BinaryValueAccess,
{
  let mut xs = Vec::with_capacity(array.len());
  let mut ys = Vec::with_capacity(array.len());
  for index in 0..array.len() {
    match array.value_opt(index) {
      Some(bytes) => {
        let (x, y) = transform
          .transform_point_from_wkb(bytes)
          .map_err(to_datafusion_error)?;
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

fn transformed_bounds_struct<T>(
  array: &T,
  transform: &PreparedTransform,
  geometry_category: GeometryCategory,
) -> DataFusionResult<StructArray>
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
        let extent = transform
          .transform_geometry_bounds_from_wkb(bytes, geometry_category)
          .map_err(to_datafusion_error)?;
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

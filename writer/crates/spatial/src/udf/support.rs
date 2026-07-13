//! Provides shared Arrow mapping, struct construction, WKB adapters, and error conversion.

use std::sync::Arc;

use arrow_array::builder::BinaryBuilder;
use arrow_array::{Array, ArrayRef, Float64Array, StructArray, UInt64Array};
use arrow_schema::DataType;
use datafusion::common::cast::{
  as_binary_array, as_binary_view_array, as_float64_array, as_large_binary_array,
};
use datafusion::common::{DataFusionError, Result as DataFusionResult};

use crate::analysis::{DisplayGeometryType, Extent2D};
use crate::output::optimized::multiscale::{
  geometry_extent_from_wkb as pbf_geometry_extent_from_wkb,
  point_xy_from_wkb as pbf_point_xy_from_wkb,
};
use crate::reprojection::PreparedTransform;

use super::signatures::{bounds_struct_fields, point_coords_fields};

/// Map nullable WKB values from any supported binary array into f64 output.
pub(super) fn map_geometry_to_f64(
  geometry: &ArrayRef,
  evaluator: impl FnMut(Option<&[u8]>) -> DataFusionResult<Option<f64>>,
) -> DataFusionResult<Float64Array> {
  match geometry.data_type() {
    DataType::Binary => map_binary_like_to_f64(as_binary_array(geometry.as_ref())?, evaluator),
    DataType::LargeBinary => {
      map_binary_like_to_f64(as_large_binary_array(geometry.as_ref())?, evaluator)
    }
    DataType::BinaryView => {
      map_binary_like_to_f64(as_binary_view_array(geometry.as_ref())?, evaluator)
    }
    other => Err(DataFusionError::Execution(format!(
      "unsupported geometry data type for UDF: {other}"
    ))),
  }
}

/// Apply an f64-producing callback to a concrete Arrow binary representation.
fn map_binary_like_to_f64<T>(
  array: &T,
  mut evaluator: impl FnMut(Option<&[u8]>) -> DataFusionResult<Option<f64>>,
) -> DataFusionResult<Float64Array>
where
  T: BinaryValueAccess,
{
  let mut values = Vec::with_capacity(array.len());
  for index in 0..array.len() {
    values.push(evaluator(array.value_opt(index))?);
  }
  Ok(Float64Array::from(values))
}

/// Map nullable WKB values from any supported binary array into u64 output.
pub(super) fn map_geometry_to_u64(
  geometry: &ArrayRef,
  evaluator: impl FnMut(Option<&[u8]>) -> DataFusionResult<u64>,
) -> DataFusionResult<UInt64Array> {
  match geometry.data_type() {
    DataType::Binary => map_binary_like_to_u64(as_binary_array(geometry.as_ref())?, evaluator),
    DataType::LargeBinary => {
      map_binary_like_to_u64(as_large_binary_array(geometry.as_ref())?, evaluator)
    }
    DataType::BinaryView => {
      map_binary_like_to_u64(as_binary_view_array(geometry.as_ref())?, evaluator)
    }
    other => Err(DataFusionError::Execution(format!(
      "unsupported geometry data type for UDF: {other}"
    ))),
  }
}

/// Apply a u64-producing callback to a concrete Arrow binary representation.
fn map_binary_like_to_u64<T>(
  array: &T,
  mut evaluator: impl FnMut(Option<&[u8]>) -> DataFusionResult<u64>,
) -> DataFusionResult<UInt64Array>
where
  T: BinaryValueAccess,
{
  let mut values = Vec::with_capacity(array.len());
  for index in 0..array.len() {
    values.push(evaluator(array.value_opt(index))?);
  }
  Ok(UInt64Array::from(values))
}

/// Map nullable WKB values while preserving their concrete Arrow binary representation.
pub(super) fn map_geometry_to_binary(
  geometry: &ArrayRef,
  evaluator: impl FnMut(Option<&[u8]>) -> DataFusionResult<Option<Vec<u8>>>,
) -> DataFusionResult<ArrayRef> {
  match geometry.data_type() {
    DataType::Binary => map_binary_like_to_binary(as_binary_array(geometry.as_ref())?, evaluator),
    DataType::LargeBinary => {
      map_binary_like_to_binary(as_large_binary_array(geometry.as_ref())?, evaluator)
    }
    DataType::BinaryView => {
      map_binary_like_to_binary(as_binary_view_array(geometry.as_ref())?, evaluator)
    }
    other => Err(DataFusionError::Execution(format!(
      "unsupported geometry data type for UDF: {other}"
    ))),
  }
}

/// Apply a binary-producing callback to a concrete Arrow binary representation.
fn map_binary_like_to_binary<T>(
  array: &T,
  mut evaluator: impl FnMut(Option<&[u8]>) -> DataFusionResult<Option<Vec<u8>>>,
) -> DataFusionResult<ArrayRef>
where
  T: BinaryValueAccess,
{
  let mut builder = BinaryBuilder::with_capacity(array.len(), array.len() * 16);
  for index in 0..array.len() {
    match evaluator(array.value_opt(index))? {
      Some(bytes) => builder.append_value(bytes),
      None => builder.append_null(),
    }
  }
  Ok(Arc::new(builder.finish()))
}

pub(super) fn feature_bbox_struct(
  geometry: &ArrayRef,
  xmin: &Float64Array,
  ymin: &Float64Array,
  xmax: &Float64Array,
  ymax: &Float64Array,
) -> DataFusionResult<StructArray> {
  StructArray::try_new(
    bounds_struct_fields(),
    vec![
      Arc::new(xmin.clone()),
      Arc::new(ymin.clone()),
      Arc::new(xmax.clone()),
      Arc::new(ymax.clone()),
    ],
    geometry.nulls().cloned(),
  )
  .map_err(to_datafusion_error)
}

pub(super) fn transformed_point_coords_struct<T>(
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
        let (x, y) = point_xy_from_wkb(bytes)?;
        let (x, y) = transform
          .transform_point(x, y)
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
    point_coords_fields(),
    vec![Arc::new(x_array.clone()), Arc::new(y_array)],
    x_array.nulls().cloned(),
  )
  .map_err(to_datafusion_error)
}

pub(super) fn transformed_bounds_struct<T>(
  array: &T,
  transform: &PreparedTransform,
  geometry_type: DisplayGeometryType,
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
          .transform_geometry_bounds_from_wkb(bytes, geometry_type)
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
    bounds_struct_fields(),
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

/// Abstracts zero-copy value access across Arrow binary, large-binary, and binary-view arrays.
pub(super) trait BinaryValueAccess {
  fn len(&self) -> usize;
  fn value_opt(&self, index: usize) -> Option<&[u8]>;
}

impl BinaryValueAccess for arrow_array::BinaryArray {
  fn len(&self) -> usize {
    Array::len(self)
  }

  fn value_opt(&self, index: usize) -> Option<&[u8]> {
    (!self.is_null(index)).then(|| self.value(index))
  }
}

impl BinaryValueAccess for arrow_array::LargeBinaryArray {
  fn len(&self) -> usize {
    Array::len(self)
  }

  fn value_opt(&self, index: usize) -> Option<&[u8]> {
    (!self.is_null(index)).then(|| self.value(index))
  }
}

impl BinaryValueAccess for arrow_array::BinaryViewArray {
  fn len(&self) -> usize {
    Array::len(self)
  }

  fn value_opt(&self, index: usize) -> Option<&[u8]> {
    self.is_valid(index).then(|| self.value(index))
  }
}

pub(super) fn point_xy_from_wkb(bytes: &[u8]) -> DataFusionResult<(f64, f64)> {
  pbf_point_xy_from_wkb(bytes).map_err(to_datafusion_error)
}

pub(super) fn extent_from_wkb(bytes: &[u8]) -> DataFusionResult<Extent2D> {
  pbf_geometry_extent_from_wkb(bytes).map_err(to_datafusion_error)
}

pub(super) fn extent_from_arg_arrays(arrays: &[ArrayRef]) -> DataFusionResult<Extent2D> {
  let xmin = array_first_f64(&arrays[1])?;
  let ymin = array_first_f64(&arrays[2])?;
  let xmax = array_first_f64(&arrays[3])?;
  let ymax = array_first_f64(&arrays[4])?;
  Ok(Extent2D {
    xmin,
    ymin,
    xmax,
    ymax,
  })
}

pub(super) fn array_first_f64(array: &ArrayRef) -> DataFusionResult<f64> {
  let array = as_float64_array(array.as_ref())?;
  if array.is_empty() || array.is_null(0) {
    return Err(DataFusionError::Execution(
      "missing full extent argument for display helper UDF".to_string(),
    ));
  }
  Ok(array.value(0))
}

pub(super) fn to_datafusion_error(err: impl Into<anyhow::Error>) -> DataFusionError {
  DataFusionError::External(err.into().into())
}

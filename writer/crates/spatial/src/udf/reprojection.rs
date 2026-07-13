//! Implements transformed point, bounds, WKB, and covering UDFs.

use std::any::Any;
use std::sync::Arc;

use arrow_array::ArrayRef;
use arrow_schema::DataType;
use datafusion::common::cast::{
  as_binary_array, as_binary_view_array, as_float64_array, as_large_binary_array,
};
use datafusion::common::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::{
  ColumnarValue, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature,
};

use crate::analysis::DisplayGeometryType;
use crate::reprojection::TransformSpec;

use super::signatures::{
  bounds_struct_fields, feature_bbox_signature, point_coords_fields, unary_geometry_signature,
};
use super::support::{
  feature_bbox_struct, map_geometry_to_binary, to_datafusion_error, transformed_bounds_struct,
  transformed_point_coords_struct,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Transforms WKB points and returns one x/y struct per row.
struct TransformedPointCoordsUdf {
  transform: TransformSpec,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Transforms WKB geometry and returns one bounds struct per row.
struct TransformedBoundsUdf {
  transform: TransformSpec,
  geometry_type: DisplayGeometryType,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Reprojects WKB values while preserving binary array representation.
struct ReprojectGeometryUdf {
  transform: TransformSpec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Builds a nullable GeoParquet covering bbox struct from scalar bounds.
struct FeatureBboxUdf;

impl ScalarUDFImpl for TransformedPointCoordsUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "display_transformed_point_coords"
  }

  fn signature(&self) -> &Signature {
    unary_geometry_signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::Struct(point_coords_fields()))
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
    "display_transformed_bounds"
  }

  fn signature(&self) -> &Signature {
    unary_geometry_signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::Struct(bounds_struct_fields()))
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
        self.geometry_type,
      )?,
      DataType::LargeBinary => transformed_bounds_struct(
        as_large_binary_array(geometry.as_ref())?,
        &prepared,
        self.geometry_type,
      )?,
      DataType::BinaryView => transformed_bounds_struct(
        as_binary_view_array(geometry.as_ref())?,
        &prepared,
        self.geometry_type,
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
    "display_reproject_geometry"
  }

  fn signature(&self) -> &Signature {
    unary_geometry_signature()
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

impl ScalarUDFImpl for FeatureBboxUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "display_feature_bbox"
  }

  fn signature(&self) -> &Signature {
    feature_bbox_signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::Struct(bounds_struct_fields()))
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let geometry = arrays
      .first()
      .ok_or_else(|| DataFusionError::Execution("missing geometry argument".to_string()))?;
    let xmin = as_float64_array(
      arrays
        .get(1)
        .ok_or_else(|| DataFusionError::Execution("missing xmin argument".to_string()))?
        .as_ref(),
    )?;
    let ymin = as_float64_array(
      arrays
        .get(2)
        .ok_or_else(|| DataFusionError::Execution("missing ymin argument".to_string()))?
        .as_ref(),
    )?;
    let xmax = as_float64_array(
      arrays
        .get(3)
        .ok_or_else(|| DataFusionError::Execution("missing xmax argument".to_string()))?
        .as_ref(),
    )?;
    let ymax = as_float64_array(
      arrays
        .get(4)
        .ok_or_else(|| DataFusionError::Execution("missing ymax argument".to_string()))?
        .as_ref(),
    )?;
    let output = feature_bbox_struct(geometry, xmin, ymin, xmax, ymax)?;
    Ok(ColumnarValue::Array(Arc::new(output) as ArrayRef))
  }
}
pub(super) fn transformed_point_coords_udf(transform: TransformSpec) -> ScalarUDF {
  ScalarUDF::new_from_impl(TransformedPointCoordsUdf { transform })
}

pub(super) fn transformed_bounds_udf(
  transform: TransformSpec,
  geometry_type: DisplayGeometryType,
) -> ScalarUDF {
  ScalarUDF::new_from_impl(TransformedBoundsUdf {
    transform,
    geometry_type,
  })
}

pub(super) fn reproject_geometry_udf(transform: TransformSpec) -> ScalarUDF {
  ScalarUDF::new_from_impl(ReprojectGeometryUdf { transform })
}

pub(super) fn feature_bbox_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(FeatureBboxUdf)
}

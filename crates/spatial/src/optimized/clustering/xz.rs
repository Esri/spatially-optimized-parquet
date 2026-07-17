//! Computes XZ hierarchy codes and DataFusion expressions for complex geometry clustering.

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

use super::ClusterKey;

/// Stores the default maximum depth of the XZ hierarchy.
pub(crate) const DEFAULT_XZ_MAX_LEVEL: u32 = 20;

/// Select the deepest XZ hierarchy level whose cell can contain a feature extent.
fn extent_xz_level(full_extent: Extent2D, feature_extent: Extent2D, max_depth: u32) -> u32 {
  let full_extent_width = full_extent.xmax - full_extent.xmin;
  let full_extent_height = full_extent.ymax - full_extent.ymin;
  let feature_width = feature_extent.xmax - feature_extent.xmin;
  let feature_height = feature_extent.ymax - feature_extent.ymin;

  if feature_width <= 0.0 || feature_height <= 0.0 {
    return 0;
  }

  let x_level = (full_extent_width / feature_width).log2();
  let y_level = (full_extent_height / feature_height).log2();
  ((x_level.min(y_level).floor() as u32) + 1).min(max_depth)
}

/// Encode a feature extent at an XZ hierarchy level that preserves spatial containment.
pub(crate) fn extent_xz_code(
  full_extent: Extent2D,
  feature_extent: Extent2D,
  max_depth: u32,
) -> ClusterKey {
  let full_extent_width = full_extent.xmax - full_extent.xmin;
  let full_extent_height = full_extent.ymax - full_extent.ymin;
  let mut level = extent_xz_level(full_extent, feature_extent, max_depth);

  let cell_width = full_extent_width / 2f64.powi(level as i32);
  let cell_height = full_extent_height / 2f64.powi(level as i32);

  let cell_x_start = ((feature_extent.xmin - full_extent.xmin) / cell_width).floor() as i32;
  let cell_x_end = ((feature_extent.xmax - full_extent.xmin) / cell_width).floor() as i32;
  let cell_y_start = ((feature_extent.ymin - full_extent.ymin) / cell_height).floor() as i32;
  let cell_y_end = ((feature_extent.ymax - full_extent.ymin) / cell_height).floor() as i32;

  let cell_count_x = cell_x_end - cell_x_start + 1;
  let cell_count_y = cell_y_end - cell_y_start + 1;

  if cell_count_x > 2 || cell_count_y > 2 {
    level = level.saturating_sub(1);
  }

  point_xz_code(
    full_extent,
    feature_extent.xmin,
    feature_extent.ymin,
    max_depth,
    Some(level),
  )
}

/// Encode a point's path through the XZ hierarchy.
///
/// `insert_level` truncates the path for extent indexing. Without it, the code reaches
/// `max_depth`.
fn point_xz_code(
  full_extent: Extent2D,
  point_x: f64,
  point_y: f64,
  max_depth: u32,
  insert_level: Option<u32>,
) -> ClusterKey {
  let insert_level = insert_level.unwrap_or(max_depth);
  let mut depth = 0;
  let mut sequence_code = 0_u64;
  let mut xmin = full_extent.xmin;
  let mut ymin = full_extent.ymin;
  let mut xmax = full_extent.xmax;
  let mut ymax = full_extent.ymax;

  while depth != insert_level {
    let center_x = (xmin + xmax) / 2.0;
    let quadrant_x = if point_x >= center_x {
      xmin = center_x;
      1
    } else {
      xmax = center_x;
      0
    };

    let center_y = (ymin + ymax) / 2.0;
    let quadrant_y = if point_y >= center_y {
      ymin = center_y;
      1
    } else {
      ymax = center_y;
      0
    };

    let quadrant_code = quadrant_x | (quadrant_y << 1);
    sequence_code += code_for_level(quadrant_code, depth, max_depth);
    depth += 1;
  }

  ClusterKey::new(sequence_code)
}

fn code_for_level(quadrant_code: u32, depth: u32, max_depth: u32) -> u64 {
  (quadrant_code as u64) * element_count(max_depth, depth) + 1
}

fn element_count(max_depth: u32, sequence_index: u32) -> u64 {
  (4u64.pow(max_depth - sequence_index) - 1) / 3
}

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
struct ComplexGeometryClusterKeyUdf;

impl ScalarUDFImpl for ComplexGeometryClusterKeyUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "clustering_complex_geometry_xzcode"
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
struct ComplexGeometryBoundsClusterKeyUdf;

impl ScalarUDFImpl for ComplexGeometryBoundsClusterKeyUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "clustering_complex_geometry_xzcode_from_bounds"
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

fn complex_geometry_xzcode_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(ComplexGeometryClusterKeyUdf)
}

fn complex_geometry_xzcode_from_bounds_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(ComplexGeometryBoundsClusterKeyUdf)
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

pub(in crate::optimized) fn complex_geometry_xzcode_from_bounds_expr(
  xmin: Expr,
  ymin: Expr,
  xmax: Expr,
  ymax: Expr,
  full_extent: Extent2D,
) -> Expr {
  complex_geometry_xzcode_from_bounds_udf()
    .call(vec![
      xmin,
      ymin,
      xmax,
      ymax,
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn extent_xz_code_matches_reference_cases() {
    let full_extent = Extent2D {
      xmin: 0.0,
      ymin: 0.0,
      xmax: 8.0,
      ymax: 8.0,
    };

    assert_eq!(
      extent_xz_code(
        full_extent,
        Extent2D {
          xmin: 0.0,
          ymin: 0.0,
          xmax: 1.0,
          ymax: 1.0,
        },
        1,
      ),
      ClusterKey::new(1)
    );
    assert_eq!(
      extent_xz_code(
        full_extent,
        Extent2D {
          xmin: 7.0,
          ymin: 7.0,
          xmax: 8.0,
          ymax: 8.0,
        },
        1,
      ),
      ClusterKey::new(4)
    );
    assert_eq!(
      extent_xz_code(
        full_extent,
        Extent2D {
          xmin: 0.0,
          ymin: 0.0,
          xmax: 0.9,
          ymax: 0.9,
        },
        2,
      ),
      ClusterKey::new(2)
    );
    assert_eq!(
      extent_xz_code(full_extent, full_extent, 3),
      ClusterKey::new(0)
    );
    assert_eq!(
      extent_xz_code(
        full_extent,
        Extent2D {
          xmin: 3.0,
          ymin: 4.2,
          xmax: 4.9,
          ymax: 4.9,
        },
        3,
      ),
      ClusterKey::new(51)
    );
  }

  #[test]
  fn extent_xz_code_uses_grid_relative_to_full_extent() {
    let full_extent = Extent2D {
      xmin: -117.3150315,
      ymin: 33.989629,
      xmax: -116.9566474,
      ymax: 34.1729119,
    };
    let feature_extent = Extent2D {
      xmin: -117.0992242,
      ymin: 34.0448088,
      xmax: -117.085404,
      ymax: 34.0545866,
    };

    assert_eq!(
      extent_xz_code(full_extent, feature_extent, 20),
      ClusterKey::new(555482436952)
    );
  }

  #[test]
  fn point_xz_code_matches_reference_quadrants() {
    let full_extent = Extent2D {
      xmin: 2.0,
      ymin: 2.0,
      xmax: 4.0,
      ymax: 4.0,
    };
    assert_eq!(
      point_xz_code(full_extent, 2.0, 2.0, 1, None),
      ClusterKey::new(1)
    );
    assert_eq!(
      point_xz_code(full_extent, 3.0, 2.0, 1, None),
      ClusterKey::new(2)
    );
    assert_eq!(
      point_xz_code(full_extent, 2.0, 3.0, 1, None),
      ClusterKey::new(3)
    );
    assert_eq!(
      point_xz_code(full_extent, 3.0, 3.0, 1, None),
      ClusterKey::new(4)
    );
  }
}

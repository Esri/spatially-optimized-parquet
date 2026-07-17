//! Computes XZ hierarchy codes and DataFusion expressions for complex geometry clustering.

#![allow(dead_code)]

use std::any::Any;
use std::sync::{Arc, OnceLock};

use arrow_array::{Array, ArrayRef, Float64Array, StructArray, UInt64Array};
use arrow_schema::{DataType, Field, Fields};
use datafusion::common::cast::as_float64_array;
use datafusion::common::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::{
  ColumnarValue, Expr, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature, TypeSignature,
  Volatility,
};
use datafusion::prelude::{col, lit};

use crate::geometry::{Extent2D, GeometryArray, geometry_signature, to_datafusion_error};
use crate::optimized::multiscale::TEMP_XZ_CODE_COLUMN;

use super::ClusterKey;

/// Defines the default maximum depth of the XZ hierarchy.
pub(crate) const DEFAULT_XZ_MAX_LEVEL: u32 = 20;

impl ClusterKey {
  /// Encode a feature extent at an XZ hierarchy level that preserves containment.
  pub(crate) fn from_xz_extent(
    full_extent: Extent2D,
    feature_extent: Extent2D,
    max_depth: u32,
  ) -> Self {
    let full_extent_width = full_extent.xmax - full_extent.xmin;
    let full_extent_height = full_extent.ymax - full_extent.ymin;
    let mut level = Self::xz_level(full_extent, feature_extent, max_depth);

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

    Self::from_xz_point(
      full_extent,
      feature_extent.xmin,
      feature_extent.ymin,
      max_depth,
      Some(level),
    )
  }

  fn xz_level(full_extent: Extent2D, feature_extent: Extent2D, max_depth: u32) -> u32 {
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

  fn from_xz_point(
    full_extent: Extent2D,
    point_x: f64,
    point_y: f64,
    max_depth: u32,
    insert_level: Option<u32>,
  ) -> Self {
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
      sequence_code += Self::code_for_xz_level(quadrant_code, depth, max_depth);
      depth += 1;
    }

    Self::new(sequence_code)
  }

  fn code_for_xz_level(quadrant_code: u32, depth: u32, max_depth: u32) -> u64 {
    (quadrant_code as u64) * Self::xz_element_count(max_depth, depth) + 1
  }

  fn xz_element_count(max_depth: u32, sequence_index: u32) -> u64 {
    (4u64.pow(max_depth - sequence_index) - 1) / 3
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct BoundsUdf;

impl BoundsUdf {
  /// Build the bounds-struct expression for one geometry column.
  pub(crate) fn expression(geometry_column: &str) -> Expr {
    Self::udf().call(vec![col(geometry_column)])
  }

  fn udf() -> ScalarUDF {
    ScalarUDF::new_from_impl(Self)
  }

  fn bounds_struct(geometry: &GeometryArray<'_>) -> DataFusionResult<StructArray> {
    let mut xmin_values = Vec::with_capacity(geometry.len());
    let mut ymin_values = Vec::with_capacity(geometry.len());
    let mut xmax_values = Vec::with_capacity(geometry.len());
    let mut ymax_values = Vec::with_capacity(geometry.len());
    for value in geometry.values() {
      match value {
        Some(bytes) => {
          let extent = Extent2D::from_wkb(bytes).map_err(to_datafusion_error)?;
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
      Self::fields(),
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

  fn fields() -> Fields {
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
}

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
    Ok(DataType::Struct(Self::fields()))
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let geometry = arrays
      .first()
      .ok_or_else(|| DataFusionError::Execution("missing geometry argument".to_string()))?;
    let geometry = GeometryArray::try_new(geometry.as_ref())?;
    let output = Self::bounds_struct(&geometry)?;
    Ok(ColumnarValue::Array(Arc::new(output) as ArrayRef))
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct ComplexGeometryClusterKeyUdf;

impl ComplexGeometryClusterKeyUdf {
  fn udf() -> ScalarUDF {
    ScalarUDF::new_from_impl(Self)
  }

  fn signature() -> &'static Signature {
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

  fn extent_from_args(arrays: &[ArrayRef], start: usize) -> DataFusionResult<Extent2D> {
    Ok(Extent2D {
      xmin: Self::first_f64(&arrays[start])?,
      ymin: Self::first_f64(&arrays[start + 1])?,
      xmax: Self::first_f64(&arrays[start + 2])?,
      ymax: Self::first_f64(&arrays[start + 3])?,
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
}

impl ScalarUDFImpl for ComplexGeometryClusterKeyUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "clustering_complex_geometry_xzcode"
  }

  fn signature(&self) -> &Signature {
    Self::signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::UInt64)
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let geometry = arrays
      .first()
      .ok_or_else(|| DataFusionError::Execution("missing geometry argument".to_string()))?;
    let full_extent = Self::extent_from_args(&arrays, 1)?;
    let geometry = GeometryArray::try_new(geometry.as_ref())?;
    let values = geometry
      .values()
      .map(|value| match value {
        Some(bytes) => Ok(
          ClusterKey::from_xz_extent(
            full_extent,
            Extent2D::from_wkb(bytes).map_err(to_datafusion_error)?,
            DEFAULT_XZ_MAX_LEVEL,
          )
          .value(),
        ),
        None => Ok(0),
      })
      .collect::<DataFusionResult<Vec<_>>>()?;
    let output = UInt64Array::from(values);
    Ok(ColumnarValue::Array(Arc::new(output) as ArrayRef))
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ComplexGeometryBoundsClusterKeyUdf;

impl ComplexGeometryBoundsClusterKeyUdf {
  /// Build the complex-geometry cluster-key expression from bounds and full extent.
  pub(crate) fn expression(
    xmin: Expr,
    ymin: Expr,
    xmax: Expr,
    ymax: Expr,
    full_extent: Extent2D,
  ) -> Expr {
    Self::udf()
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

  fn udf() -> ScalarUDF {
    ScalarUDF::new_from_impl(Self)
  }

  fn signature() -> &'static Signature {
    static SIGNATURE: OnceLock<Signature> = OnceLock::new();
    SIGNATURE.get_or_init(|| Signature::exact(vec![DataType::Float64; 8], Volatility::Immutable))
  }

  fn extent_from_args(arrays: &[ArrayRef], start: usize) -> DataFusionResult<Extent2D> {
    Ok(Extent2D {
      xmin: Self::first_f64(&arrays[start])?,
      ymin: Self::first_f64(&arrays[start + 1])?,
      xmax: Self::first_f64(&arrays[start + 2])?,
      ymax: Self::first_f64(&arrays[start + 3])?,
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
}

impl ScalarUDFImpl for ComplexGeometryBoundsClusterKeyUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "clustering_complex_geometry_xzcode_from_bounds"
  }

  fn signature(&self) -> &Signature {
    Self::signature()
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
    let full_extent = Self::extent_from_args(&arrays, 4)?;
    let mut values = Vec::with_capacity(xmin.len());
    for index in 0..xmin.len() {
      values.push(
        if xmin.is_null(index) || ymin.is_null(index) || xmax.is_null(index) || ymax.is_null(index)
        {
          0
        } else {
          ClusterKey::from_xz_extent(
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
      ClusterKey::from_xz_extent(
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
      ClusterKey::from_xz_extent(
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
      ClusterKey::from_xz_extent(
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
      ClusterKey::from_xz_extent(full_extent, full_extent, 3),
      ClusterKey::new(0)
    );
    assert_eq!(
      ClusterKey::from_xz_extent(
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
      ClusterKey::from_xz_extent(full_extent, feature_extent, 20),
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
      ClusterKey::from_xz_point(full_extent, 2.0, 2.0, 1, None),
      ClusterKey::new(1)
    );
    assert_eq!(
      ClusterKey::from_xz_point(full_extent, 3.0, 2.0, 1, None),
      ClusterKey::new(2)
    );
    assert_eq!(
      ClusterKey::from_xz_point(full_extent, 2.0, 3.0, 1, None),
      ClusterKey::new(3)
    );
    assert_eq!(
      ClusterKey::from_xz_point(full_extent, 3.0, 3.0, 1, None),
      ClusterKey::new(4)
    );
  }
}

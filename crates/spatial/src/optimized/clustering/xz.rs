// Copyright 2026 Esri
//
// Licensed under the Apache License Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Generates XZ-order keys for spatially extended objects.
//!
//! This follows Böhm, Klump, and Kriegel, “XZ-Ordering: A Space-Filling Curve for Objects
//! with Spatial Extension” (1999). The paper enlarges every Z-order element to twice its
//! width and height toward the upper-right corner. A feature can then use one integer key
//! for the smallest enlarged element containing its bounding extent.
//!
//! Paper terms map to this implementation as follows:
//!
//! - An **element** is an [`Extent2D`] at one level of the Z-order hierarchy.
//! - The paper's resolution `g` is `max_depth` or `cluster_depth`.
//! - The quadrant-sequence length is `level` or `depth`.
//! - [`ClusterKey::from_xz_extent`] implements insertion from section 4.1.
//! - `code_for_xz_level` implements one term of the sequence code from definition 2.
//! - `xz_element_count` implements the element count from lemma 3.
//!
//! Query-side interval generation lives in `viewer/src/maplibre/sop/xz.ts`.

#![allow(dead_code)]

use std::sync::{Arc, OnceLock};

use arrow_array::{Array, ArrayRef, Float64Array, StructArray, UInt64Array};
use arrow_schema::{DataType, Field, Fields};
use datafusion::common::cast::as_float64_array;
use datafusion::common::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::{
  ColumnarValue, Expr, ReturnFieldArgs, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature,
  TypeSignature, Volatility,
};
use datafusion::prelude::{col, lit};

use crate::geometry::{Extent2D, GeometryArray, geometry_signature, to_datafusion_error};
use crate::optimized::multiscale::GEOKEY_COLUMN;

use super::ClusterKey;

impl ClusterKey {
  /// Assign one XZ key to a feature's bounding extent.
  ///
  /// This implements the insertion algorithm from section 4.1 of the paper. It chooses the
  /// smallest enlarged element that contains `feature_extent`, then encodes that element's
  /// Z-order quadrant sequence with the sequence code from definition 2.
  ///
  /// First, estimate the finer candidate level allowed by lemma 1:
  ///
  /// ```text
  /// floor(min(log2(W / w), log2(H / h))) + 1
  /// ```
  ///
  /// `W` and `H` are the full extent dimensions. `w` and `h` are the feature dimensions.
  /// Grid alignment decides whether that level fits. If the feature touches more than two
  /// cells along either axis, use the parent level instead.
  ///
  /// Finally, locate the feature's lower-left corner in the selected Z-order grid and encode
  /// its quadrant sequence. The key identifies the containing enlarged element, not the exact
  /// feature bounds.
  ///
  /// # Example
  ///
  /// ```ignore
  /// let full_extent = Extent2D { xmin: 0.0, ymin: 0.0, xmax: 8.0, ymax: 8.0 };
  /// let feature_extent = Extent2D { xmin: 6.8, ymin: 0.0, xmax: 7.9, ymax: 0.1 };
  ///
  /// let key = ClusterKey::from_xz_extent(full_extent, feature_extent, 3);
  ///
  /// assert_eq!(key, ClusterKey::new(29));
  /// ```
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

  /// Calculate the finer candidate sequence length from lemma 1.
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

  /// Encode the Z-order quadrant sequence for an element's lower-left corner.
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

  /// Add one quadrant term from definition 2's sequence-code formula.
  fn code_for_xz_level(quadrant_code: u32, depth: u32, max_depth: u32) -> u64 {
    (quadrant_code as u64) * Self::xz_element_count(max_depth, depth) + 1
  }

  /// Count an element and all descendants through `max_depth`, following lemma 3.
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

  /// Create the DataFusion scalar function used by the bounds expression.
  fn udf() -> ScalarUDF {
    ScalarUDF::new_from_impl(Self)
  }

  /// Decode geometry bounds into an Arrow struct array.
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

  /// Return the shared Arrow fields for the bounds struct.
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
  /// Return the SQL-visible function name.
  fn name(&self) -> &str {
    "clustering_bounds"
  }

  /// Accept the supported geometry binary types.
  fn signature(&self) -> &Signature {
    geometry_signature()
  }

  /// Return the bounds struct type.
  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::Struct(Self::fields()))
  }

  /// Decode each geometry into its bounding extent.
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
struct ComplexGeometryClusterKeyUdf {
  cluster_depth: u32,
}

impl ComplexGeometryClusterKeyUdf {
  /// Create the geometry-based XZ key scalar function.
  fn udf(cluster_depth: u32) -> ScalarUDF {
    ScalarUDF::new_from_impl(Self { cluster_depth })
  }

  /// Return the accepted geometry and full-extent argument types.
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

  /// Read one extent from four scalar function arguments.
  fn extent_from_args(arrays: &[ArrayRef], start: usize) -> DataFusionResult<Extent2D> {
    Ok(Extent2D {
      xmin: Self::first_f64(&arrays[start])?,
      ymin: Self::first_f64(&arrays[start + 1])?,
      xmax: Self::first_f64(&arrays[start + 2])?,
      ymax: Self::first_f64(&arrays[start + 3])?,
    })
  }

  /// Read the first non-null floating-point scalar from an argument array.
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
  /// Return the SQL-visible function name.
  fn name(&self) -> &str {
    "clustering_complex_geometry_xzcode"
  }

  /// Return the geometry-based XZ function signature.
  fn signature(&self) -> &Signature {
    Self::signature()
  }

  /// Return the unsigned XZ key type.
  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::UInt64)
  }

  /// Return the non-nullable XZ key field.
  fn return_field_from_args(&self, _: ReturnFieldArgs) -> DataFusionResult<Arc<Field>> {
    Ok(Arc::new(Field::new(self.name(), DataType::UInt64, false)))
  }

  /// Decode geometries and assign one XZ key to each bounding extent.
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
            self.cluster_depth,
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
pub(crate) struct ComplexGeometryBoundsClusterKeyUdf {
  cluster_depth: u32,
}

impl ComplexGeometryBoundsClusterKeyUdf {
  /// Build the complex-geometry cluster-key expression from bounds and full extent.
  pub(crate) fn expression(
    xmin: Expr,
    ymin: Expr,
    xmax: Expr,
    ymax: Expr,
    full_extent: Extent2D,
    cluster_depth: u32,
  ) -> Expr {
    Self::udf(cluster_depth)
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
      .alias(GEOKEY_COLUMN)
  }

  /// Create the bounds-based XZ key scalar function.
  fn udf(cluster_depth: u32) -> ScalarUDF {
    ScalarUDF::new_from_impl(Self { cluster_depth })
  }

  /// Return the eight floating-point bounds argument types.
  fn signature() -> &'static Signature {
    static SIGNATURE: OnceLock<Signature> = OnceLock::new();
    SIGNATURE.get_or_init(|| Signature::exact(vec![DataType::Float64; 8], Volatility::Immutable))
  }

  /// Read one extent from four scalar function arguments.
  fn extent_from_args(arrays: &[ArrayRef], start: usize) -> DataFusionResult<Extent2D> {
    Ok(Extent2D {
      xmin: Self::first_f64(&arrays[start])?,
      ymin: Self::first_f64(&arrays[start + 1])?,
      xmax: Self::first_f64(&arrays[start + 2])?,
      ymax: Self::first_f64(&arrays[start + 3])?,
    })
  }

  /// Read the first non-null floating-point scalar from an argument array.
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
  /// Return the SQL-visible function name.
  fn name(&self) -> &str {
    "clustering_complex_geometry_xzcode_from_bounds"
  }

  /// Return the bounds-based XZ function signature.
  fn signature(&self) -> &Signature {
    Self::signature()
  }

  /// Return the unsigned XZ key type.
  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::UInt64)
  }

  /// Return the non-nullable XZ key field.
  fn return_field_from_args(&self, _: ReturnFieldArgs) -> DataFusionResult<Arc<Field>> {
    Ok(Arc::new(Field::new(self.name(), DataType::UInt64, false)))
  }

  /// Assign one XZ key to each supplied feature extent.
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
            self.cluster_depth,
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

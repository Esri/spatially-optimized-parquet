//! Computes Morton Z-order codes and DataFusion expressions for point clustering.

use std::any::Any;
use std::sync::{Arc, OnceLock};

use arrow_array::{Array, ArrayRef, Float64Array, StructArray, UInt64Array};
use arrow_schema::{DataType, Field, Fields};
use datafusion::common::cast::as_float64_array;
use datafusion::common::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::{
  ColumnarValue, Expr, ReturnFieldArgs, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature,
  Volatility,
};
use datafusion::prelude::{col, lit};

use crate::geometry::WkbCoordinate;
use crate::geometry::{Extent2D, GeometryArray, geometry_signature, to_datafusion_error};
use crate::optimized::multiscale::{
  GEOKEY_COLUMN, POINT_M_COLUMN, POINT_X_COLUMN, POINT_Y_COLUMN, POINT_Z_COLUMN,
};

use super::ClusterKey;

/// Defines the default number of quantization bits per point coordinate axis.
pub(crate) const DEFAULT_COORDINATE_PRECISION: u32 = 20;

impl ClusterKey {
  /// Encode a point within the full extent through interleaved x/y quantization bits.
  pub(crate) fn from_z_coordinates(
    full_extent: Extent2D,
    x: f64,
    y: f64,
    coordinate_precision: u32,
  ) -> Self {
    let quantized_x =
      Self::quantize_to_bits(x, full_extent.xmin, full_extent.xmax, coordinate_precision);
    let quantized_y =
      Self::quantize_to_bits(y, full_extent.ymin, full_extent.ymax, coordinate_precision);
    Self::swizzle_bits(quantized_x, quantized_y, coordinate_precision)
  }

  fn swizzle_bits(x: u32, y: u32, coordinate_precision: u32) -> Self {
    let mut code = 0;
    for bit in 0..coordinate_precision.min(32) {
      let x_bit = ((x >> bit) & 1) as u64;
      let y_bit = ((y >> bit) & 1) as u64;
      code |= x_bit << (2 * bit);
      code |= y_bit << (2 * bit + 1);
    }
    Self::new(code)
  }

  fn quantize_to_bits(value: f64, min: f64, max: f64, coordinate_precision: u32) -> u32 {
    if coordinate_precision == 0 || (max - min).abs() < f64::EPSILON {
      return 0;
    }
    let cell_count = 1u64 << coordinate_precision.min(32);
    let normalized = (value - min) / (max - min);
    let quantized = (normalized * cell_count as f64) as i64;
    quantized.clamp(0, cell_count as i64 - 1) as u32
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PointGeometryUdf {
  expected_dimensions: Option<(bool, bool)>,
}

impl PointGeometryUdf {
  /// Build a point-coordinate expression without source-dimension validation.
  pub(crate) fn expression(geometry_column: &str) -> Expr {
    Self::new(None).udf().call(vec![col(geometry_column)])
  }

  /// Build a point-coordinate expression with expected source dimensions.
  pub(crate) fn expression_with_dimensions(
    geometry_column: &str,
    has_z: bool,
    has_m: bool,
  ) -> Expr {
    Self::new(Some((has_z, has_m)))
      .udf()
      .call(vec![col(geometry_column)])
  }

  fn new(expected_dimensions: Option<(bool, bool)>) -> Self {
    Self {
      expected_dimensions,
    }
  }

  fn udf(self) -> ScalarUDF {
    ScalarUDF::new_from_impl(self)
  }

  fn coordinate_struct(
    geometry: &GeometryArray<'_>,
    expected_dimensions: Option<(bool, bool)>,
  ) -> DataFusionResult<StructArray> {
    let mut xs = Vec::with_capacity(geometry.len());
    let mut ys = Vec::with_capacity(geometry.len());
    let mut zs = Vec::with_capacity(geometry.len());
    let mut ms = Vec::with_capacity(geometry.len());
    for value in geometry.values() {
      let Some(bytes) = value else {
        xs.push(None);
        ys.push(None);
        zs.push(None);
        ms.push(None);
        continue;
      };
      let coordinate = WkbCoordinate::from_point_wkb(bytes).map_err(to_datafusion_error)?;
      if let Some((has_z, has_m)) = expected_dimensions
        && (coordinate.z.is_some() != has_z || coordinate.m.is_some() != has_m)
      {
        return Err(DataFusionError::Execution(
          "point WKB dimensions do not match GeoParquet metadata".to_string(),
        ));
      }
      xs.push(Some(coordinate.x));
      ys.push(Some(coordinate.y));
      zs.push(coordinate.z);
      ms.push(coordinate.m);
    }
    StructArray::try_new(
      Self::fields(),
      vec![
        Arc::new(Float64Array::from(xs)),
        Arc::new(Float64Array::from(ys)),
        Arc::new(Float64Array::from(zs)),
        Arc::new(Float64Array::from(ms)),
      ],
      None,
    )
    .map_err(to_datafusion_error)
  }

  fn fields() -> Fields {
    static FIELDS: OnceLock<Fields> = OnceLock::new();
    FIELDS
      .get_or_init(|| {
        Fields::from(vec![
          Arc::new(Field::new(POINT_X_COLUMN, DataType::Float64, true)),
          Arc::new(Field::new(POINT_Y_COLUMN, DataType::Float64, true)),
          Arc::new(Field::new(POINT_Z_COLUMN, DataType::Float64, true)),
          Arc::new(Field::new(POINT_M_COLUMN, DataType::Float64, true)),
        ])
      })
      .clone()
  }
}

impl ScalarUDFImpl for PointGeometryUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "clustering_point_geometry"
  }

  fn signature(&self) -> &Signature {
    geometry_signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::Struct(Self::fields()))
  }

  fn return_field_from_args(&self, _: ReturnFieldArgs) -> DataFusionResult<Arc<Field>> {
    Ok(Arc::new(Field::new(
      self.name(),
      DataType::Struct(Self::fields()),
      true,
    )))
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let geometry = arrays
      .first()
      .ok_or_else(|| DataFusionError::Execution("missing geometry argument".to_string()))?;
    let geometry = GeometryArray::try_new(geometry.as_ref())?;
    let output = Self::coordinate_struct(&geometry, self.expected_dimensions)?;
    Ok(ColumnarValue::Array(Arc::new(output) as ArrayRef))
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct PointGeometryClusterKeyUdf;

impl PointGeometryClusterKeyUdf {
  /// Build the point cluster-key expression from x/y values and the full extent.
  pub(crate) fn expression(x: Expr, y: Expr, full_extent: Extent2D) -> Expr {
    Self::udf()
      .call(vec![
        x,
        y,
        lit(full_extent.xmin),
        lit(full_extent.ymin),
        lit(full_extent.xmax),
        lit(full_extent.ymax),
      ])
      .alias(GEOKEY_COLUMN)
  }

  fn udf() -> ScalarUDF {
    ScalarUDF::new_from_impl(Self)
  }

  fn signature() -> &'static Signature {
    static SIGNATURE: OnceLock<Signature> = OnceLock::new();
    SIGNATURE.get_or_init(|| Signature::exact(vec![DataType::Float64; 6], Volatility::Immutable))
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
        "missing full extent argument for Z clustering".to_string(),
      ));
    }
    Ok(array.value(0))
  }
}

impl ScalarUDFImpl for PointGeometryClusterKeyUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "clustering_point_geometry_zcode_from_xy"
  }

  fn signature(&self) -> &Signature {
    Self::signature()
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
    let full_extent = Self::extent_from_args(&arrays, 2)?;
    let mut values = Vec::with_capacity(x.len());
    for index in 0..x.len() {
      values.push(if x.is_null(index) || y.is_null(index) {
        0
      } else {
        ClusterKey::from_z_coordinates(
          full_extent,
          x.value(index),
          y.value(index),
          DEFAULT_COORDINATE_PRECISION,
        )
        .value()
      });
    }
    Ok(ColumnarValue::Array(
      Arc::new(UInt64Array::from_iter_values(values)) as ArrayRef,
    ))
  }
}

#[cfg(test)]
mod tests {
  use arrow_array::BinaryArray;
  use geo_types::{Geometry, Point};

  use super::*;

  #[test]
  fn point_coordinates_are_null_for_missing_geometry() {
    let point_wkb = crate::geometry::write_test_geometry(&Geometry::Point(Point::new(1.0, 2.0)));
    let input = BinaryArray::from(vec![Some(point_wkb.as_slice()), None]);

    let geometry = GeometryArray::try_new(&input).unwrap();
    let coordinates = PointGeometryUdf::coordinate_struct(&geometry, None).unwrap();
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

    assert!(coordinates.fields()[0].is_nullable());
    assert!(coordinates.fields()[1].is_nullable());
    assert_eq!(coordinates.null_count(), 0);
    assert_eq!(x.null_count(), 1);
    assert_eq!(y.null_count(), 1);
    assert_eq!(x.value(0), 1.0);
    assert_eq!(y.value(0), 2.0);
    assert!(x.is_null(1));
    assert!(y.is_null(1));
  }

  #[test]
  fn point_coordinate_extraction_rejects_invalid_wkb() {
    let invalid_wkb = [0_u8, 1, 2];
    let input = BinaryArray::from(vec![Some(invalid_wkb.as_slice())]);

    let geometry = GeometryArray::try_new(&input).unwrap();
    let error = PointGeometryUdf::coordinate_struct(&geometry, None).unwrap_err();

    assert!(error.to_string().contains("unexpected end of WKB"));
  }

  #[test]
  fn point_z_code_normalizes_against_full_extent() {
    let full_extent = Extent2D {
      xmin: -180.0,
      ymin: -90.0,
      xmax: 180.0,
      ymax: 90.0,
    };

    assert_eq!(
      ClusterKey::from_z_coordinates(full_extent, -180.0, -90.0, 4),
      ClusterKey::new(0)
    );
    assert_eq!(
      ClusterKey::from_z_coordinates(full_extent, 180.0, 90.0, 2),
      ClusterKey::new(15)
    );
    assert_eq!(
      ClusterKey::from_z_coordinates(full_extent, 0.0, 0.0, 1),
      ClusterKey::new(3)
    );
  }

  #[test]
  fn point_z_code_uses_cell_quantization() {
    let full_extent = Extent2D {
      xmin: 0.0,
      ymin: 0.0,
      xmax: 1.0,
      ymax: 1.0,
    };

    assert_eq!(
      ClusterKey::from_z_coordinates(full_extent, 0.2, 0.0, 2),
      ClusterKey::new(0)
    );
    assert_eq!(
      ClusterKey::from_z_coordinates(full_extent, 0.25, 0.0, 2),
      ClusterKey::new(1)
    );
    assert_eq!(
      ClusterKey::from_z_coordinates(full_extent, 1.0, 1.0, 2),
      ClusterKey::new(15)
    );
  }
}

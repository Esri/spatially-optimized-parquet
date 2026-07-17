//! Computes Morton Z-order codes and DataFusion expressions for point clustering.

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
  Volatility,
};
use datafusion::prelude::{col, lit};

use crate::geometry::read_wkb_point_coordinate;
use crate::geometry::{BinaryValueAccess, Extent2D, geometry_signature, to_datafusion_error};
use crate::optimized::multiscale::{
  POINT_M_COLUMN, POINT_X_COLUMN, POINT_Y_COLUMN, POINT_Z_CODE_COLUMN, POINT_Z_COLUMN,
};

use super::ClusterKey;

/// Stores the default number of quantization bits per point coordinate axis.
pub(crate) const DEFAULT_COORDINATE_PRECISION: u32 = 20;

/// Quantize a point within the full extent and interleave its x/y bits.
pub(crate) fn point_z_code(
  full_extent: Extent2D,
  x: f64,
  y: f64,
  coordinate_precision: u32,
) -> ClusterKey {
  let quantized_x = quantize_to_bits(x, full_extent.xmin, full_extent.xmax, coordinate_precision);
  let quantized_y = quantize_to_bits(y, full_extent.ymin, full_extent.ymax, coordinate_precision);
  swizzle_bits(quantized_x, quantized_y, coordinate_precision)
}

/// Interleave x and y bits into one Morton-order code.
fn swizzle_bits(x: u32, y: u32, coordinate_precision: u32) -> ClusterKey {
  let mut code = 0;
  for bit in 0..coordinate_precision.min(32) {
    let x_bit = ((x >> bit) & 1) as u64;
    let y_bit = ((y >> bit) & 1) as u64;
    code |= x_bit << (2 * bit);
    code |= y_bit << (2 * bit + 1);
  }
  ClusterKey::new(code)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PointGeometryUdf {
  expected_dimensions: Option<(bool, bool)>,
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
    Ok(DataType::Struct(point_fields()))
  }

  fn return_field_from_args(&self, _: ReturnFieldArgs) -> DataFusionResult<Arc<Field>> {
    Ok(Arc::new(Field::new(
      self.name(),
      DataType::Struct(point_fields()),
      true,
    )))
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let geometry = arrays
      .first()
      .ok_or_else(|| DataFusionError::Execution("missing geometry argument".to_string()))?;
    let output = match geometry.data_type() {
      DataType::Binary => point_coords_struct(
        as_binary_array(geometry.as_ref())?,
        self.expected_dimensions,
      )?,
      DataType::LargeBinary => point_coords_struct(
        as_large_binary_array(geometry.as_ref())?,
        self.expected_dimensions,
      )?,
      DataType::BinaryView => point_coords_struct(
        as_binary_view_array(geometry.as_ref())?,
        self.expected_dimensions,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PointGeometryClusterKeyUdf;

impl ScalarUDFImpl for PointGeometryClusterKeyUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "clustering_point_geometry_zcode_from_xy"
  }

  fn signature(&self) -> &Signature {
    z_cluster_signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::UInt64)
  }

  fn return_field_from_args(&self, _: ReturnFieldArgs) -> DataFusionResult<Arc<Field>> {
    Ok(Arc::new(Field::new(self.name(), DataType::UInt64, true)))
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let x = as_float64_array(arrays[0].as_ref())?;
    let y = as_float64_array(arrays[1].as_ref())?;
    let full_extent = extent_from_args(&arrays, 2)?;
    let mut values = Vec::with_capacity(x.len());
    for index in 0..x.len() {
      values.push(if x.is_null(index) || y.is_null(index) {
        None
      } else {
        Some(
          point_z_code(
            full_extent,
            x.value(index),
            y.value(index),
            DEFAULT_COORDINATE_PRECISION,
          )
          .value(),
        )
      });
    }
    Ok(ColumnarValue::Array(
      Arc::new(UInt64Array::from(values)) as ArrayRef
    ))
  }
}

fn point_udf(expected_dimensions: Option<(bool, bool)>) -> ScalarUDF {
  ScalarUDF::new_from_impl(PointGeometryUdf {
    expected_dimensions,
  })
}

fn point_geometry_zcode_from_xy_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(PointGeometryClusterKeyUdf)
}

fn point_coords_struct<T>(
  array: &T,
  expected_dimensions: Option<(bool, bool)>,
) -> DataFusionResult<StructArray>
where
  T: BinaryValueAccess,
{
  let mut xs = Vec::with_capacity(array.len());
  let mut ys = Vec::with_capacity(array.len());
  let mut zs = Vec::with_capacity(array.len());
  let mut ms = Vec::with_capacity(array.len());
  for index in 0..array.len() {
    let Some(bytes) = array.value_opt(index) else {
      xs.push(None);
      ys.push(None);
      zs.push(None);
      ms.push(None);
      continue;
    };
    let coordinate = read_wkb_point_coordinate(bytes).map_err(to_datafusion_error)?;
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
    point_fields(),
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

fn point_fields() -> Fields {
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

fn z_cluster_signature() -> &'static Signature {
  static SIGNATURE: OnceLock<Signature> = OnceLock::new();
  SIGNATURE.get_or_init(|| Signature::exact(vec![DataType::Float64; 6], Volatility::Immutable))
}

pub(crate) fn point_expr(geometry_column: &str) -> Expr {
  point_udf(None).call(vec![col(geometry_column)])
}

pub(crate) fn point_geometry_expr_with_dimensions(
  geometry_column: &str,
  has_z: bool,
  has_m: bool,
) -> Expr {
  point_udf(Some((has_z, has_m))).call(vec![col(geometry_column)])
}

pub(crate) fn point_geometry_zcode_from_xy_expr(x: Expr, y: Expr, full_extent: Extent2D) -> Expr {
  point_geometry_zcode_from_xy_udf()
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
  fn point_coordinates_are_null_for_missing_geometry() {
    let point_wkb = crate::geometry::write_test_geometry(&Geometry::Point(Point::new(1.0, 2.0)));
    let input = BinaryArray::from(vec![Some(point_wkb.as_slice()), None]);

    let coordinates = point_coords_struct(&input, None).unwrap();
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

    let error = point_coords_struct(&input, None).unwrap_err();

    assert!(error.to_string().contains("unexpected end of WKB"));
  }

  #[test]
  fn swizzle_bits_interleaves_xy_bits() {
    assert_eq!(swizzle_bits(0, 0, 4), ClusterKey::new(0));
    assert_eq!(swizzle_bits(1, 0, 4), ClusterKey::new(1));
    assert_eq!(swizzle_bits(0, 1, 4), ClusterKey::new(2));
    assert_eq!(swizzle_bits(1, 1, 4), ClusterKey::new(3));
    assert_eq!(swizzle_bits(3, 3, 2), ClusterKey::new(15));
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
      point_z_code(full_extent, -180.0, -90.0, 4),
      ClusterKey::new(0)
    );
    assert_eq!(
      point_z_code(full_extent, 180.0, 90.0, 2),
      ClusterKey::new(15)
    );
    assert_eq!(point_z_code(full_extent, 0.0, 0.0, 1), ClusterKey::new(3));
  }

  #[test]
  fn point_z_code_uses_cell_quantization() {
    let full_extent = Extent2D {
      xmin: 0.0,
      ymin: 0.0,
      xmax: 1.0,
      ymax: 1.0,
    };

    assert_eq!(point_z_code(full_extent, 0.2, 0.0, 2), ClusterKey::new(0));
    assert_eq!(point_z_code(full_extent, 0.25, 0.0, 2), ClusterKey::new(1));
    assert_eq!(point_z_code(full_extent, 1.0, 1.0, 2), ClusterKey::new(15));
  }
}

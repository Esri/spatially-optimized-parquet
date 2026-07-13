//! Implements point, bounds, Z-order, and XZ-order UDFs.

use std::any::Any;
use std::sync::Arc;

use arrow_array::{Array, ArrayRef, UInt64Array};
use arrow_schema::DataType;
use datafusion::common::cast::as_float64_array;
use datafusion::common::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::{
  ColumnarValue, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature,
};

use crate::analysis::Extent2D;
use crate::output::optimized::clustering::xz::{DEFAULT_XZ_MAX_LEVEL, extent_xz_code};
use crate::output::optimized::clustering::z::{DEFAULT_COORDINATE_PRECISION, point_z_code};

use super::signatures::{
  code_geometry_signature, non_point_xzcode_from_bounds_signature, point_zcode_from_xy_signature,
  unary_geometry_signature,
};
use super::support::{
  array_first_f64, extent_from_arg_arrays, extent_from_wkb, map_geometry_to_f64,
  map_geometry_to_u64, point_xy_from_wkb,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Selects the scalar extracted by a unary WKB-to-f64 UDF.
enum FloatUdfKind {
  PointX,
  PointY,
  BoundsXmin,
  BoundsYmin,
  BoundsXmax,
  BoundsYmax,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Implements point-coordinate and geometry-bound scalar extraction.
struct GeometryFloatUdf {
  name: &'static str,
  kind: FloatUdfKind,
}

impl ScalarUDFImpl for GeometryFloatUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    self.name
  }

  fn signature(&self) -> &Signature {
    unary_geometry_signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::Float64)
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let geometry = arrays
      .first()
      .ok_or_else(|| DataFusionError::Execution("missing geometry argument".to_string()))?;
    let output = match self.kind {
      FloatUdfKind::PointX => map_geometry_to_f64(geometry, |bytes| {
        Ok(Some(
          bytes
            .map(point_xy_from_wkb)
            .transpose()?
            .map(|(x, _)| x)
            .unwrap_or(f64::NAN),
        ))
      })?,
      FloatUdfKind::PointY => map_geometry_to_f64(geometry, |bytes| {
        Ok(Some(
          bytes
            .map(point_xy_from_wkb)
            .transpose()?
            .map(|(_, y)| y)
            .unwrap_or(f64::NAN),
        ))
      })?,
      FloatUdfKind::BoundsXmin => map_geometry_to_f64(geometry, |bytes| {
        Ok(
          bytes
            .map(extent_from_wkb)
            .transpose()?
            .map(|extent| extent.xmin),
        )
      })?,
      FloatUdfKind::BoundsYmin => map_geometry_to_f64(geometry, |bytes| {
        Ok(
          bytes
            .map(extent_from_wkb)
            .transpose()?
            .map(|extent| extent.ymin),
        )
      })?,
      FloatUdfKind::BoundsXmax => map_geometry_to_f64(geometry, |bytes| {
        Ok(
          bytes
            .map(extent_from_wkb)
            .transpose()?
            .map(|extent| extent.xmax),
        )
      })?,
      FloatUdfKind::BoundsYmax => map_geometry_to_f64(geometry, |bytes| {
        Ok(
          bytes
            .map(extent_from_wkb)
            .transpose()?
            .map(|extent| extent.ymax),
        )
      })?,
    };
    Ok(ColumnarValue::Array(Arc::new(output) as ArrayRef))
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Selects the spatial clustering algorithm.
enum Cluster {
  Z,
  XZ,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Resolves a cluster key from prepared coordinate or bounds columns.
struct ClusterKeyUdf {
  name: &'static str,
  cluster: Cluster,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Resolves a cluster key directly from WKB geometry.
struct GeometryClusterKeyUdf {
  name: &'static str,
  cluster: Cluster,
}

impl ScalarUDFImpl for GeometryClusterKeyUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    self.name
  }

  fn signature(&self) -> &Signature {
    code_geometry_signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::UInt64)
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let geometry = arrays
      .first()
      .ok_or_else(|| DataFusionError::Execution("missing geometry argument".to_string()))?;
    let full_extent = extent_from_arg_arrays(&arrays)?;
    let output = map_geometry_to_u64(geometry, |bytes| match (self.cluster, bytes) {
      (Cluster::Z, Some(bytes)) => {
        let (x, y) = point_xy_from_wkb(bytes)?;
        Ok(point_z_code(
          full_extent,
          x,
          y,
          DEFAULT_COORDINATE_PRECISION,
        ))
      }
      (Cluster::XZ, Some(bytes)) => Ok(extent_xz_code(
        full_extent,
        extent_from_wkb(bytes)?,
        DEFAULT_XZ_MAX_LEVEL,
      )),
      (_, None) => Ok(0),
    })?;
    Ok(ColumnarValue::Array(Arc::new(output) as ArrayRef))
  }
}

impl ScalarUDFImpl for ClusterKeyUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    self.name
  }

  fn signature(&self) -> &Signature {
    match self.cluster {
      Cluster::Z => point_zcode_from_xy_signature(),
      Cluster::XZ => non_point_xzcode_from_bounds_signature(),
    }
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::UInt64)
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let output = match self.cluster {
      Cluster::Z => {
        let x = as_float64_array(arrays[0].as_ref())?;
        let y = as_float64_array(arrays[1].as_ref())?;
        let full_extent = Extent2D {
          xmin: array_first_f64(&arrays[2])?,
          ymin: array_first_f64(&arrays[3])?,
          xmax: array_first_f64(&arrays[4])?,
          ymax: array_first_f64(&arrays[5])?,
        };
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
          });
        }
        UInt64Array::from(values)
      }
      Cluster::XZ => {
        let xmin = as_float64_array(arrays[0].as_ref())?;
        let ymin = as_float64_array(arrays[1].as_ref())?;
        let xmax = as_float64_array(arrays[2].as_ref())?;
        let ymax = as_float64_array(arrays[3].as_ref())?;
        let full_extent = Extent2D {
          xmin: array_first_f64(&arrays[4])?,
          ymin: array_first_f64(&arrays[5])?,
          xmax: array_first_f64(&arrays[6])?,
          ymax: array_first_f64(&arrays[7])?,
        };
        let mut values = Vec::with_capacity(xmin.len());
        for index in 0..xmin.len() {
          values.push(
            if xmin.is_null(index)
              || ymin.is_null(index)
              || xmax.is_null(index)
              || ymax.is_null(index)
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
            },
          );
        }
        UInt64Array::from(values)
      }
    };
    Ok(ColumnarValue::Array(Arc::new(output) as ArrayRef))
  }
}

pub(super) fn point_zcode_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(GeometryClusterKeyUdf {
    name: "display_point_zcode",
    cluster: Cluster::Z,
  })
}

pub(super) fn non_point_xzcode_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(GeometryClusterKeyUdf {
    name: "display_nonpoint_xzcode",
    cluster: Cluster::XZ,
  })
}

pub(super) fn point_x_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(GeometryFloatUdf {
    name: "display_point_x",
    kind: FloatUdfKind::PointX,
  })
}

pub(super) fn point_y_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(GeometryFloatUdf {
    name: "display_point_y",
    kind: FloatUdfKind::PointY,
  })
}

pub(super) fn bounds_xmin_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(GeometryFloatUdf {
    name: "display_bounds_xmin",
    kind: FloatUdfKind::BoundsXmin,
  })
}

pub(super) fn bounds_ymin_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(GeometryFloatUdf {
    name: "display_bounds_ymin",
    kind: FloatUdfKind::BoundsYmin,
  })
}

pub(super) fn bounds_xmax_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(GeometryFloatUdf {
    name: "display_bounds_xmax",
    kind: FloatUdfKind::BoundsXmax,
  })
}

pub(super) fn bounds_ymax_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(GeometryFloatUdf {
    name: "display_bounds_ymax",
    kind: FloatUdfKind::BoundsYmax,
  })
}
pub(super) fn point_zcode_from_xy_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(ClusterKeyUdf {
    name: "display_point_zcode_from_xy",
    cluster: Cluster::Z,
  })
}

pub(super) fn non_point_xzcode_from_bounds_udf() -> ScalarUDF {
  ScalarUDF::new_from_impl(ClusterKeyUdf {
    name: "display_nonpoint_xzcode_from_bounds",
    cluster: Cluster::XZ,
  })
}

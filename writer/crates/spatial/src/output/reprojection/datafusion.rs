//! Implements typed DataFusion expressions for geometry reprojection.

use arrow_schema::DataType;
use datafusion::common::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::{
  ColumnarValue, Expr, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature,
};
use datafusion::prelude::col;
use std::any::Any;

use crate::geometry::{geometry_signature, map_geometry_to_binary, to_datafusion_error};

use super::CoordinateTransformSpec;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ReprojectGeometryUdf {
  transform: CoordinateTransformSpec,
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

fn reproject_geometry_udf(transform: CoordinateTransformSpec) -> ScalarUDF {
  ScalarUDF::new_from_impl(ReprojectGeometryUdf { transform })
}

pub(crate) fn reproject_geometry_expr(
  geometry_column: &str,
  transform: &CoordinateTransformSpec,
) -> Expr {
  reproject_geometry_udf(transform.clone()).call(vec![col(geometry_column)])
}

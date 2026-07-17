//! Implements typed DataFusion expressions for geometry reprojection.

use arrow_array::ArrayRef;
use arrow_array::builder::BinaryBuilder;
use arrow_schema::DataType;
use datafusion::common::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::{
  ColumnarValue, Expr, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature,
};
use datafusion::prelude::col;
use std::any::Any;
use std::sync::Arc;

use crate::geometry::{GeometryArray, geometry_signature, to_datafusion_error};

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
    let geometry = GeometryArray::try_new(geometry.as_ref())?;
    let mut builder = BinaryBuilder::with_capacity(geometry.len(), geometry.len() * 16);
    for value in geometry.values() {
      match value {
        Some(bytes) => {
          builder.append_value(prepared.reproject_wkb(bytes).map_err(to_datafusion_error)?)
        }
        None => builder.append_null(),
      }
    }
    Ok(ColumnarValue::Array(Arc::new(builder.finish()) as ArrayRef))
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

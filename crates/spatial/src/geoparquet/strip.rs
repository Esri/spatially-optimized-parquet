//! Builds lazy WKB dimension-stripping expressions for normalized GeoParquet geometry.

use std::any::Any;
use std::sync::Arc;

use arrow_array::ArrayRef;
use arrow_array::builder::BinaryBuilder;
use arrow_schema::DataType;
use datafusion::common::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::{
  ColumnarValue, Expr, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature,
};
use datafusion::prelude::col;

use crate::geometry::{
  GeometryArray, geometry_signature, strip_wkb_dimensions, to_datafusion_error,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct StripGeometryDimensionsUdf {
  strip_z: bool,
  strip_m: bool,
}

impl ScalarUDFImpl for StripGeometryDimensionsUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn name(&self) -> &str {
    "strip_geometry_dimensions"
  }

  fn signature(&self) -> &Signature {
    geometry_signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::Binary)
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let geometry = ColumnarValue::values_to_arrays(&args.args)?
      .into_iter()
      .next()
      .ok_or_else(|| DataFusionError::Execution("missing geometry argument".to_string()))?;
    let geometry = GeometryArray::try_new(geometry.as_ref())?;
    let mut builder = BinaryBuilder::with_capacity(geometry.len(), geometry.len() * 16);
    for value in geometry.values() {
      match value {
        Some(bytes) => builder.append_value(
          strip_wkb_dimensions(bytes, self.strip_z, self.strip_m).map_err(to_datafusion_error)?,
        ),
        None => builder.append_null(),
      }
    }
    Ok(ColumnarValue::Array(Arc::new(builder.finish()) as ArrayRef))
  }
}

pub(crate) fn strip_geometry_dimensions_expr(
  geometry_column: &str,
  strip_z: bool,
  strip_m: bool,
) -> Expr {
  ScalarUDF::new_from_impl(StripGeometryDimensionsUdf { strip_z, strip_m })
    .call(vec![col(geometry_column)])
}

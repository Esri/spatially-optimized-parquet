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

//! Builds lazy WKB dimension-stripping expressions for normalized spatial geometry.

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
pub(crate) struct StripGeometryDimensionsUdf {
  strip_z: bool,
  strip_m: bool,
}

impl StripGeometryDimensionsUdf {
  pub(crate) fn expression(geometry_column: &str, strip_z: bool, strip_m: bool) -> Expr {
    ScalarUDF::new_from_impl(Self { strip_z, strip_m }).call(vec![col(geometry_column)])
  }
}

impl ScalarUDFImpl for StripGeometryDimensionsUdf {
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

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

//! Creates GeoParquet covering bbox structs through typed DataFusion expressions.

use std::sync::{Arc, OnceLock};

use arrow_array::{ArrayRef, Float64Array, StructArray};
use arrow_schema::{DataType, Field, Fields};
use datafusion::common::cast::as_float64_array;
use datafusion::common::{DataFusionError, Result as DataFusionResult};
use datafusion::functions::core::expr_ext::FieldAccessor;
use datafusion::logical_expr::{
  ColumnarValue, Expr, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature, TypeSignature,
  Volatility,
};
use datafusion::prelude::col;

use crate::geometry::GeometryFamily;
use crate::geometry::to_datafusion_error;
use crate::optimized::{BoundsUdf, PointGeometryUdf};

/// Identifies the canonical GeoParquet bounding-box covering column.
pub(crate) const COVERING_BBOX_COLUMN: &str = "bbox";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FeatureBboxUdf;

impl FeatureBboxUdf {
  fn scalar_udf() -> ScalarUDF {
    ScalarUDF::new_from_impl(Self)
  }

  fn float_argument<'a>(
    arrays: &'a [ArrayRef],
    index: usize,
    name: &str,
  ) -> DataFusionResult<&'a Float64Array> {
    as_float64_array(
      arrays
        .get(index)
        .ok_or_else(|| DataFusionError::Execution(format!("missing {name} argument")))?
        .as_ref(),
    )
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
}

impl ScalarUDFImpl for FeatureBboxUdf {
  fn name(&self) -> &str {
    "geoparquet_feature_bbox"
  }

  fn signature(&self) -> &Signature {
    Self::signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::Struct(Self::bounds_fields()))
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let geometry = arrays
      .first()
      .ok_or_else(|| DataFusionError::Execution("missing geometry argument".to_string()))?;
    let xmin = Self::float_argument(&arrays, 1, "xmin")?;
    let ymin = Self::float_argument(&arrays, 2, "ymin")?;
    let xmax = Self::float_argument(&arrays, 3, "xmax")?;
    let ymax = Self::float_argument(&arrays, 4, "ymax")?;
    let nulls = geometry.nulls().cloned();
    let output = StructArray::try_new(
      Self::bounds_fields(),
      vec![
        Arc::new(Float64Array::new(xmin.values().clone(), nulls.clone())),
        Arc::new(Float64Array::new(ymin.values().clone(), nulls.clone())),
        Arc::new(Float64Array::new(xmax.values().clone(), nulls.clone())),
        Arc::new(Float64Array::new(ymax.values().clone(), nulls.clone())),
      ],
      nulls,
    )
    .map_err(to_datafusion_error)?;
    Ok(ColumnarValue::Array(Arc::new(output) as ArrayRef))
  }
}

pub(crate) fn geometry_bbox_expr(geometry_column: &str, geometry_family: GeometryFamily) -> Expr {
  let geometry = col(geometry_column);
  match geometry_family {
    GeometryFamily::Point => {
      let coordinates = PointGeometryUdf::expression(geometry_column);
      let x = coordinates.clone().field("x");
      let y = coordinates.field("y");
      FeatureBboxUdf::scalar_udf()
        .call(vec![geometry, x.clone(), y.clone(), x, y])
        .alias(COVERING_BBOX_COLUMN)
    }
    GeometryFamily::MultiPoint | GeometryFamily::Polyline | GeometryFamily::Polygon => {
      let bounds = BoundsUdf::expression(geometry_column);
      FeatureBboxUdf::scalar_udf()
        .call(vec![
          geometry,
          bounds.clone().field("xmin"),
          bounds.clone().field("ymin"),
          bounds.clone().field("xmax"),
          bounds.field("ymax"),
        ])
        .alias(COVERING_BBOX_COLUMN)
    }
  }
}

pub(crate) fn bbox_field_expr(field: &str) -> Expr {
  col(COVERING_BBOX_COLUMN).field(field)
}

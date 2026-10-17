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

//! Normalizes spatial columns before extent analysis and output projection.
//!
//! Reprojects geometry when required, creates one canonical GeoParquet bbox column, and records
//! the canonical geometry state consumed by optimized output.

use arrow_schema::{DataType, Schema};
use datafusion::dataframe::DataFrame;
use datafusion::logical_expr::expr_fn::ident;

use crate::geoparquet::{COVERING_BBOX_COLUMN, geometry_bbox_expr};
use crate::pipeline::{
  PipelineError, ResolvedReprojection, ResolvedSpatialSource, StripGeometryDimensionsUdf,
};

#[derive(Clone)]
pub(crate) struct NormalizedSpatialFrame {
  dataframe: DataFrame,
  geometry_column: String,
}

impl NormalizedSpatialFrame {
  /// Normalize output WKB and attach reusable or computed canonical XY covering bounds.
  pub(crate) fn new(
    mut dataframe: DataFrame,
    source_schema: &Schema,
    source: &ResolvedSpatialSource,
    reprojection: &ResolvedReprojection,
    strip_z: bool,
    strip_m: bool,
  ) -> Result<Self, PipelineError> {
    if let Some(expression) = reprojection.output_geometry_expr(&source.geometry.column)? {
      dataframe = dataframe
        .with_column(&source.geometry.column, expression)
        .map_err(|source| PipelineError::DataFusion {
          operation: "normalize output geometry column",
          source,
        })?;
    }
    if strip_z || strip_m {
      dataframe = dataframe
        .with_column(
          &source.geometry.column,
          StripGeometryDimensionsUdf::expression(&source.geometry.column, strip_z, strip_m),
        )
        .map_err(|source| PipelineError::DataFusion {
          operation: "strip geometry dimensions",
          source,
        })?;
    }

    let reusable_covering = if reprojection.requires_reprojection() {
      None
    } else {
      source
        .source_metadata
        .geometry
        .as_ref()
        .and_then(|geometry| geometry.covering.as_ref())
        .filter(|covering| valid_bbox_field(source_schema, &covering.column))
    };
    dataframe = if let Some(covering) = reusable_covering {
      dataframe
        .with_column(COVERING_BBOX_COLUMN, ident(&covering.column))
        .map_err(|source| PipelineError::DataFusion {
          operation: "reuse source covering column",
          source,
        })?
    } else {
      dataframe
        .with_column(
          COVERING_BBOX_COLUMN,
          geometry_bbox_expr(&source.geometry.column, source.geometry_family),
        )
        .map_err(|source| PipelineError::DataFusion {
          operation: "compute GeoParquet covering column",
          source,
        })?
    };

    Ok(Self {
      dataframe,
      geometry_column: source.geometry.column.clone(),
    })
  }

  /// Clone the lazy frame for extent analysis or output execution.
  pub(crate) fn dataframe(&self) -> DataFrame {
    self.dataframe.clone()
  }

  /// Return the normalized WKB column name.
  pub(crate) fn geometry_column(&self) -> &str {
    &self.geometry_column
  }
}

/// Require a struct containing all four canonical Float64 XY bounds fields.
fn valid_bbox_field(schema: &Schema, column: &str) -> bool {
  let Ok(field) = schema.field_with_name(column) else {
    return false;
  };
  let DataType::Struct(fields) = field.data_type() else {
    return false;
  };
  for name in ["xmin", "ymin", "xmax", "ymax"] {
    if !fields
      .find(name)
      .is_some_and(|(_, field)| field.data_type() == &DataType::Float64)
    {
      return false;
    }
  }
  true
}

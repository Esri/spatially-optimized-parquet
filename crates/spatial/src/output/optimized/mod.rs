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

//! Owns persisted spatially optimized GeoParquet output.

use arrow_schema::Schema;
use datafusion::logical_expr::{Expr, expr_fn::ident};

use crate::geoparquet::COVERING_BBOX_COLUMN;
use crate::optimized::OptimizedLayout;
use crate::output::{OutputError, WriterOptions};
use crate::pipeline::SpatialWriteContext;

pub(crate) mod partitioned;
pub(crate) mod single;

/// Select source columns followed by the canonical bounds required for optimized clustering.
fn source_expressions(source_schema: &Schema) -> Vec<Expr> {
  let mut expressions = Vec::with_capacity(source_schema.fields().len() + 1);
  for field in source_schema.fields() {
    if field.name() != COVERING_BBOX_COLUMN {
      expressions.push(ident(field.name()));
    }
  }
  expressions.push(ident(COVERING_BBOX_COLUMN));
  expressions
}

/// Configure optimized metadata, coordinate encoding, and the primary WKB field for writing.
fn writer_options(
  context: &SpatialWriteContext,
  layout: &OptimizedLayout,
  covering: bool,
  compression: Option<&str>,
) -> Result<WriterOptions, OutputError> {
  let metadata = layout
    .parquet_metadata(context, covering)
    .map_err(|error| OutputError::Configuration(format!("build optimized metadata: {error}")))?;
  let geometry_crs = format!(
    "srid:{}",
    context
      .reprojection()
      .target_spatial_reference()
      .wkid
      .ok_or_else(|| {
        OutputError::Configuration("missing output spatial-reference WKID".to_string())
      })?
  );
  Ok(
    WriterOptions::new(compression.unwrap_or("snappy"), &metadata)?
      .with_byte_stream_split_columns(layout.byte_stream_split_column_paths())
      .with_geometry_column(&layout.geometry().geometry.column, geometry_crs),
  )
}

#[cfg(test)]
mod tests {
  use arrow_schema::{DataType, Field, Schema};

  use super::source_expressions;

  #[test]
  fn source_selection_retains_order_and_moves_covering_last() {
    let schema = Schema::new(vec![
      Field::new("name", DataType::Utf8, true),
      Field::new("bbox", DataType::Binary, true),
      Field::new("geometry", DataType::Binary, true),
    ]);
    let selected = source_expressions(&schema);
    let expected = ["name", "geometry", "bbox"];
    for (expression, name) in selected.iter().zip(expected) {
      assert_eq!(expression, &datafusion::logical_expr::expr_fn::ident(name));
    }
    assert_eq!(selected.len(), expected.len());
  }

  #[test]
  fn source_selection_adds_covering_when_source_schema_omits_it() {
    let schema = Schema::new(vec![Field::new("geometry", DataType::Binary, true)]);
    assert_eq!(
      source_expressions(&schema),
      vec![
        datafusion::logical_expr::expr_fn::ident("geometry"),
        datafusion::logical_expr::expr_fn::ident("bbox"),
      ]
    );
  }
}

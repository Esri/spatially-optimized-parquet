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

//! Builds range-assigned dataframes for partitioned optimized output.

use datafusion::dataframe::DataFrame;
use datafusion::logical_expr::expr_fn::ident;

use crate::geoparquet::COVERING_BBOX_COLUMN;
use crate::optimized::{ClusterRangeBoundaries, OptimizedLayout};
use crate::output::OutputError;
use crate::pipeline::{PipelineWarnings, SpatialWriteContext};

/// Build the narrow cluster-key dataframe consumed by partition-boundary analysis.
pub(super) fn range_source(
  context: &SpatialWriteContext,
  layout: &OptimizedLayout,
) -> Result<DataFrame, OutputError> {
  let dataframe = context
    .frame()
    .dataframe()
    .select(vec![
      ident(&layout.geometry().geometry.column),
      ident(COVERING_BBOX_COLUMN),
    ])
    .map_err(|source| OutputError::DataFusion {
      operation: "select cluster-range source columns",
      source,
    })?;
  layout
    .geometry()
    .clustering_dataframe(dataframe, context.target_extent(), layout.cluster_depth())
    .map_err(|error| OutputError::Configuration(format!("build cluster-range source: {error}")))
}

/// Build optimized output with one range partition value per row.
pub(super) fn dataframe(
  source_schema: &arrow_schema::Schema,
  context: &SpatialWriteContext,
  layout: &OptimizedLayout,
  boundaries: &ClusterRangeBoundaries,
  covering: bool,
  warnings: PipelineWarnings,
) -> Result<DataFrame, OutputError> {
  let dataframe = context
    .frame()
    .dataframe()
    .select(
      source_schema
        .fields()
        .iter()
        .filter(|field| field.name() != COVERING_BBOX_COLUMN)
        .map(|field| ident(field.name()))
        .chain(std::iter::once(ident(COVERING_BBOX_COLUMN)))
        .collect::<Vec<_>>(),
    )
    .map_err(|source| OutputError::DataFusion {
      operation: "select partitioned output columns",
      source,
    })?;
  let clustering_family = layout.geometry().clustering_family;
  let partition_column = clustering_family.cluster_partition_column();
  let dataframe = layout
    .geometry()
    .clustering_dataframe(dataframe, context.target_extent(), layout.cluster_depth())
    .map_err(|error| {
      OutputError::Configuration(format!("build partitioned clustering data: {error}"))
    })?
    .with_column(
      partition_column,
      boundaries
        .partition_expr(clustering_family.cluster_key_column())
        .map_err(|error| {
          OutputError::Configuration(format!("build range partition expression: {error}"))
        })?,
    )
    .map_err(|source| OutputError::DataFusion {
      operation: "add output range partition column",
      source,
    })?;
  let mut expressions = layout.output_expressions(source_schema, covering, warnings);
  expressions.push(ident(partition_column));
  dataframe
    .select(expressions)
    .map_err(|source| OutputError::DataFusion {
      operation: "select partitioned output expressions",
      source,
    })
}

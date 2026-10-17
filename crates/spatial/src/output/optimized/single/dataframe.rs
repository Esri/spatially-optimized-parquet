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

//! Builds the globally sorted dataframe for single-file optimized output.

use crate::optimized::OptimizedLayout;
use crate::output::OutputError;
use crate::pipeline::{PipelineWarnings, SpatialWriteContext};
use datafusion::dataframe::DataFrame;

/// Build globally sorted optimized output while retaining the cluster key for the physical sink.
pub(super) fn dataframe(
  source_schema: &arrow_schema::Schema,
  context: &SpatialWriteContext,
  layout: &OptimizedLayout,
  covering: bool,
  warnings: PipelineWarnings,
) -> Result<DataFrame, OutputError> {
  let dataframe = context
    .frame()
    .dataframe()
    .select(super::super::source_expressions(source_schema))
    .map_err(|source| OutputError::DataFusion {
      operation: "select optimized output columns",
      source,
    })?;
  let clustering_family = layout.geometry().clustering_family;
  let dataframe = layout
    .geometry()
    .clustering_dataframe(dataframe, context.target_extent(), layout.cluster_depth())
    .map_err(|error| {
      OutputError::Configuration(format!("build optimized clustering data: {error}"))
    })?
    .sort(vec![clustering_family.sort_expr()])
    .map_err(|source| OutputError::DataFusion {
      operation: "sort optimized output",
      source,
    })?;
  dataframe
    .select(layout.output_expressions(source_schema, covering, warnings))
    .map_err(|source| OutputError::DataFusion {
      operation: "select optimized output expressions",
      source,
    })
}

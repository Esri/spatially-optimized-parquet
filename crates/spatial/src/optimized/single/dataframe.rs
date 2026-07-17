//! Builds the globally sorted dataframe for single-file optimized output.

use anyhow::Result;
use datafusion::dataframe::DataFrame;
use datafusion::logical_expr::expr_fn::ident;

use crate::optimized::ResolvedOptimization;
use crate::optimized::multiscale::COVERING_BBOX_COLUMN;
use crate::pipeline::PipelineWarnings;

/// Build globally sorted optimized output while retaining the cluster key for the physical sink.
pub(super) fn dataframe(
  input_dataframe: DataFrame,
  source_schema: &arrow_schema::Schema,
  optimization: &ResolvedOptimization,
  covering: bool,
  warnings: PipelineWarnings,
) -> Result<DataFrame> {
  let dataframe = input_dataframe.select(
    source_schema
      .fields()
      .iter()
      .filter(|field| field.name() != COVERING_BBOX_COLUMN)
      .map(|field| ident(field.name()))
      .chain(std::iter::once(ident(COVERING_BBOX_COLUMN)))
      .collect::<Vec<_>>(),
  )?;
  let clustering_family = optimization.geometry().clustering_family;
  let dataframe = optimization
    .geometry()
    .clustering_dataframe(dataframe, optimization.target_extent())?
    .sort(vec![clustering_family.sort_expr()])?;
  let mut expressions = optimization.output_expressions(source_schema, covering, warnings);
  expressions.push(ident(clustering_family.cluster_key_column()));
  dataframe.select(expressions).map_err(Into::into)
}

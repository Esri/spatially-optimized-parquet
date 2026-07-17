//! Builds the globally sorted dataframe for single-file optimized output.

use anyhow::Result;
use datafusion::dataframe::DataFrame;
use datafusion::logical_expr::expr_fn::ident;

use crate::optimized::ResolvedOptimization;
use crate::optimized::clustering::{cluster_key_column, cluster_sort_expr, clustering_dataframe};
use crate::optimized::multiscale::COVERING_BBOX_COLUMN;
use crate::optimized::select::output_expressions;
use crate::pipeline::PipelineWarningStore;

/// Build globally sorted optimized output while retaining the cluster key for the physical sink.
pub(super) fn dataframe(
  input_dataframe: DataFrame,
  source_schema: &arrow_schema::Schema,
  optimization: &ResolvedOptimization,
  covering: bool,
  warning_store: PipelineWarningStore,
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
  let dataframe = clustering_dataframe(
    dataframe,
    optimization.geometry(),
    optimization.target_extent(),
  )?
  .sort(vec![cluster_sort_expr(
    optimization.geometry().clustering_family,
  )])?;
  let mut expressions = output_expressions(source_schema, optimization, covering, warning_store);
  expressions.push(ident(cluster_key_column(
    optimization.geometry().clustering_family,
  )));
  dataframe.select(expressions).map_err(Into::into)
}

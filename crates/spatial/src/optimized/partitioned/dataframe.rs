//! Builds range-assigned dataframes for partitioned optimized output.

use anyhow::Result;
use datafusion::dataframe::DataFrame;
use datafusion::logical_expr::expr_fn::ident;

use crate::optimized::ResolvedOptimization;
use crate::optimized::clustering::{
  ClusterRangeBoundaries, cluster_key_column, cluster_partition_column, clustering_dataframe,
};
use crate::optimized::multiscale::COVERING_BBOX_COLUMN;
use crate::optimized::select::output_expressions;
use crate::pipeline::PipelineWarningStore;

/// Build the narrow cluster-key dataframe consumed by partition-boundary analysis.
pub(super) fn range_source(
  input_dataframe: DataFrame,
  optimization: &ResolvedOptimization,
) -> Result<DataFrame> {
  let dataframe = input_dataframe.select(vec![
    ident(&optimization.geometry().geometry_spec.column),
    ident(COVERING_BBOX_COLUMN),
  ])?;
  clustering_dataframe(
    dataframe,
    optimization.geometry(),
    optimization.target_extent(),
  )
}

/// Build optimized output with one range partition value per row.
pub(super) fn dataframe(
  input_dataframe: DataFrame,
  source_schema: &arrow_schema::Schema,
  optimization: &ResolvedOptimization,
  boundaries: &ClusterRangeBoundaries,
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
  let partition_column = cluster_partition_column(optimization.geometry().clustering_family);
  let dataframe = clustering_dataframe(
    dataframe,
    optimization.geometry(),
    optimization.target_extent(),
  )?
  .with_column(
    partition_column,
    boundaries.partition_expr(cluster_key_column(
      optimization.geometry().clustering_family,
    ))?,
  )?;
  let mut expressions = output_expressions(source_schema, optimization, covering, warning_store);
  expressions.push(ident(partition_column));
  expressions.push(ident(cluster_key_column(
    optimization.geometry().clustering_family,
  )));
  dataframe.select(expressions).map_err(Into::into)
}

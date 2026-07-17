//! Builds range-assigned dataframes for partitioned optimized output.

use anyhow::Result;
use datafusion::dataframe::DataFrame;
use datafusion::logical_expr::expr_fn::ident;

use crate::optimized::ResolvedOptimization;
use crate::optimized::clustering::ClusterRangeBoundaries;
use crate::optimized::multiscale::COVERING_BBOX_COLUMN;
use crate::pipeline::PipelineWarnings;

/// Build the narrow cluster-key dataframe consumed by partition-boundary analysis.
pub(super) fn range_source(
  input_dataframe: DataFrame,
  optimization: &ResolvedOptimization,
) -> Result<DataFrame> {
  let dataframe = input_dataframe.select(vec![
    ident(&optimization.geometry().geometry.column),
    ident(COVERING_BBOX_COLUMN),
  ])?;
  optimization
    .geometry()
    .clustering_dataframe(dataframe, optimization.target_extent())
}

/// Build optimized output with one range partition value per row.
pub(super) fn dataframe(
  input_dataframe: DataFrame,
  source_schema: &arrow_schema::Schema,
  optimization: &ResolvedOptimization,
  boundaries: &ClusterRangeBoundaries,
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
  let partition_column = clustering_family.cluster_partition_column();
  let dataframe = optimization
    .geometry()
    .clustering_dataframe(dataframe, optimization.target_extent())?
    .with_column(
      partition_column,
      boundaries.partition_expr(clustering_family.cluster_key_column())?,
    )?;
  let mut expressions = optimization.output_expressions(source_schema, covering, warnings);
  expressions.push(ident(partition_column));
  expressions.push(ident(clustering_family.cluster_key_column()));
  dataframe.select(expressions).map_err(Into::into)
}

//! Builds range-assigned dataframes for partitioned optimized output.

use anyhow::Result;
use datafusion::dataframe::DataFrame;
use datafusion::logical_expr::expr_fn::ident;

use crate::geoparquet::{COVERING_BBOX_COLUMN, GeoParquetWriteContext};
use crate::optimized::OptimizedLayout;
use crate::optimized::clustering::ClusterRangeBoundaries;
use crate::pipeline::PipelineWarnings;

/// Build the narrow cluster-key dataframe consumed by partition-boundary analysis.
pub(super) fn range_source(
  context: &GeoParquetWriteContext,
  layout: &OptimizedLayout,
) -> Result<DataFrame> {
  let dataframe = context.frame().dataframe().select(vec![
    ident(&layout.geometry().geometry.column),
    ident(COVERING_BBOX_COLUMN),
  ])?;
  layout
    .geometry()
    .clustering_dataframe(dataframe, context.target_extent())
}

/// Build optimized output with one range partition value per row.
pub(super) fn dataframe(
  source_schema: &arrow_schema::Schema,
  context: &GeoParquetWriteContext,
  layout: &OptimizedLayout,
  boundaries: &ClusterRangeBoundaries,
  covering: bool,
  warnings: PipelineWarnings,
) -> Result<DataFrame> {
  let dataframe = context.frame().dataframe().select(
    source_schema
      .fields()
      .iter()
      .filter(|field| field.name() != COVERING_BBOX_COLUMN)
      .map(|field| ident(field.name()))
      .chain(std::iter::once(ident(COVERING_BBOX_COLUMN)))
      .collect::<Vec<_>>(),
  )?;
  let clustering_family = layout.geometry().clustering_family;
  let partition_column = clustering_family.cluster_partition_column();
  let dataframe = layout
    .geometry()
    .clustering_dataframe(dataframe, context.target_extent())?
    .with_column(
      partition_column,
      boundaries.partition_expr(clustering_family.cluster_key_column())?,
    )?;
  let mut expressions = layout.output_expressions(source_schema, covering, warnings);
  expressions.push(ident(partition_column));
  expressions.push(ident(clustering_family.cluster_key_column()));
  dataframe.select(expressions).map_err(Into::into)
}

//! Builds the globally sorted dataframe for single-file optimized output.

use anyhow::Result;
use datafusion::dataframe::DataFrame;
use datafusion::logical_expr::expr_fn::ident;

use crate::geoparquet::COVERING_BBOX_COLUMN;
use crate::optimized::OptimizedLayout;
use crate::pipeline::{PipelineWarnings, SpatialWriteContext};

/// Build globally sorted optimized output while retaining the cluster key for the physical sink.
pub(super) fn dataframe(
  source_schema: &arrow_schema::Schema,
  context: &SpatialWriteContext,
  layout: &OptimizedLayout,
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
  let dataframe = layout
    .geometry()
    .clustering_dataframe(dataframe, context.target_extent(), layout.cluster_depth())?
    .sort(vec![clustering_family.sort_expr()])?;
  dataframe
    .select(layout.output_expressions(source_schema, covering, warnings))
    .map_err(Into::into)
}

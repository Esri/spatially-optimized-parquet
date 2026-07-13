//! Resolves the optimized extent in target coordinates.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use arrow_array::{Array, Float64Array, RecordBatch};
use datafusion::functions::core::expr_ext::FieldAccessor;
use datafusion::functions_aggregate::expr_fn::{max, min};
use datafusion::logical_expr::{Expr, expr_fn::ident};

use crate::diagnostics::{explain_stage_note, explain_timing};
use crate::geometry::Extent2D;
use crate::geoparquet::SourceGeoParquetContext;
use crate::input::materialized::input_dataframe_for_job;
use crate::optimized::clustering::{bounds_expr, point_expr};
use crate::optimized::execution::collect_dataframe_with_metric_polling;
use crate::optimized::multiscale::{
  POINT_X_COLUMN, POINT_Y_COLUMN, TEMP_BOUNDS_COLUMN, TEMP_POINT_COORDS_COLUMN, TEMP_XMAX_COLUMN,
  TEMP_XMIN_COLUMN, TEMP_YMAX_COLUMN, TEMP_YMIN_COLUMN,
};
use crate::optimized::{ClusteringFamily, OptimizedGeometry};
use crate::output::reprojection::{
  ReprojectionContext, transformed_bounds_expr, transformed_point_coords_expr,
};
use crate::output::stage::OutputStageContext;
use crate::progress::{finish_row_bar, row_bar};

/// Resolve the selected-row extent in the output coordinate reference system.
pub(crate) async fn resolve_target_extent(
  request: &OutputStageContext<'_>,
  source: &SourceGeoParquetContext,
  geometry: &OptimizedGeometry,
  reprojection: &ReprojectionContext,
) -> Result<Extent2D> {
  let progress_bar = row_bar(
    request.progress,
    "Analyzing geometry",
    request.total_input_rows,
  );
  let source_metadata = request.input.source_metadata()?;
  let metadata_fast_path = request.row_range.is_full()
    && !reprojection.requires_reprojection()
    && source_metadata
      .geometry
      .as_ref()
      .filter(|metadata| metadata.column == geometry.geometry_spec.column)
      .and_then(|metadata| metadata.bbox)
      .is_some();
  let target_extent = if metadata_fast_path {
    explain_stage_note(
      request.explain,
      "Analyzing geometry",
      "using metadata fast path from resolved source metadata",
    );
    explain_timing(request.explain, "Analyzing geometry", Duration::ZERO);
    progress_bar.inc(request.total_input_rows);
    source.source_extent
  } else {
    let dataframe = input_dataframe_for_job(
      request.input,
      request.session,
      request.row_range,
      request.materialized_batches,
    )
    .await?;
    let aggregate_dataframe = build_target_extent_aggregate(dataframe, geometry, reprojection)?;
    let batches = collect_dataframe_with_metric_polling(
      aggregate_dataframe,
      &progress_bar,
      request.total_input_rows,
      "Analyzing geometry",
      request.explain,
    )
    .await?;
    extract_target_extent(&batches)?
  };
  finish_row_bar(
    &progress_bar,
    request.total_input_rows,
    format!("Analyzed {} geometry", geometry.geometry_type.as_str()),
  );
  Ok(target_extent)
}

fn build_target_extent_aggregate(
  dataframe: engine::DataFrame,
  geometry: &OptimizedGeometry,
  reprojection: &ReprojectionContext,
) -> Result<engine::DataFrame> {
  let dataframe = match geometry.clustering_family {
    ClusteringFamily::Point => {
      let coordinates = match reprojection.transform() {
        Some(transform) => transformed_point_coords_expr(&geometry.geometry_spec.column, transform),
        None => point_expr(&geometry.geometry_spec.column),
      };
      dataframe
        .with_column(TEMP_POINT_COORDS_COLUMN, coordinates)?
        .select(vec![
          ident(TEMP_POINT_COORDS_COLUMN)
            .field("x")
            .alias(POINT_X_COLUMN),
          ident(TEMP_POINT_COORDS_COLUMN)
            .field("y")
            .alias(POINT_Y_COLUMN),
        ])?
    }
    ClusteringFamily::NonPoint => {
      let bounds = match reprojection.transform() {
        Some(transform) => transformed_bounds_expr(
          &geometry.geometry_spec.column,
          geometry.geometry_type.category(),
          transform,
        ),
        None => bounds_expr(&geometry.geometry_spec.column),
      };
      dataframe
        .with_column(TEMP_BOUNDS_COLUMN, bounds)?
        .select(vec![
          ident(TEMP_BOUNDS_COLUMN)
            .field("xmin")
            .alias(TEMP_XMIN_COLUMN),
          ident(TEMP_BOUNDS_COLUMN)
            .field("ymin")
            .alias(TEMP_YMIN_COLUMN),
          ident(TEMP_BOUNDS_COLUMN)
            .field("xmax")
            .alias(TEMP_XMAX_COLUMN),
          ident(TEMP_BOUNDS_COLUMN)
            .field("ymax")
            .alias(TEMP_YMAX_COLUMN),
        ])?
    }
  };
  let aggregate_expressions = match geometry.clustering_family {
    ClusteringFamily::Point => extent_aggregate_expressions(
      POINT_X_COLUMN,
      POINT_Y_COLUMN,
      POINT_X_COLUMN,
      POINT_Y_COLUMN,
    ),
    ClusteringFamily::NonPoint => extent_aggregate_expressions(
      TEMP_XMIN_COLUMN,
      TEMP_YMIN_COLUMN,
      TEMP_XMAX_COLUMN,
      TEMP_YMAX_COLUMN,
    ),
  };
  dataframe
    .aggregate(vec![], aggregate_expressions)
    .map_err(Into::into)
}

fn extent_aggregate_expressions(
  xmin_column: &str,
  ymin_column: &str,
  xmax_column: &str,
  ymax_column: &str,
) -> Vec<Expr> {
  vec![
    min(ident(xmin_column)).alias(TEMP_XMIN_COLUMN),
    min(ident(ymin_column)).alias(TEMP_YMIN_COLUMN),
    max(ident(xmax_column)).alias(TEMP_XMAX_COLUMN),
    max(ident(ymax_column)).alias(TEMP_YMAX_COLUMN),
  ]
}

fn extract_target_extent(batches: &[RecordBatch]) -> Result<Extent2D> {
  let Some(batch) = batches.first().filter(|batch| batch.num_rows() > 0) else {
    bail!("unable to determine dataset target extent");
  };
  Ok(Extent2D {
    xmin: extract_aggregate_value(batch, 0, "xmin")?,
    ymin: extract_aggregate_value(batch, 1, "ymin")?,
    xmax: extract_aggregate_value(batch, 2, "xmax")?,
    ymax: extract_aggregate_value(batch, 3, "ymax")?,
  })
}

fn extract_aggregate_value(batch: &RecordBatch, column_index: usize, label: &str) -> Result<f64> {
  let values = batch
    .column(column_index)
    .as_any()
    .downcast_ref::<Float64Array>()
    .with_context(|| format!("target extent aggregate column '{label}' was not Float64"))?;
  if values.is_null(0) {
    bail!("unable to determine dataset target extent");
  }
  Ok(values.value(0))
}

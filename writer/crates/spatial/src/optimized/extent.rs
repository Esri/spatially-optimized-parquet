//! Resolves the optimized extent in target coordinates.

use anyhow::{Context, Result, bail};
use arrow_array::{Array, Float64Array, RecordBatch};
use datafusion::dataframe::DataFrame;
use datafusion::functions::core::expr_ext::FieldAccessor;
use datafusion::functions_aggregate::expr_fn::{max, min};
use datafusion::logical_expr::{Expr, expr_fn::ident};

use crate::geometry::Extent2D;
use crate::geoparquet::ResolvedGeoParquetSource;
use crate::input::{InputSource, RowRange};
use crate::optimized::clustering::{bounds_expr, point_expr};
use crate::optimized::multiscale::{
  POINT_X_COLUMN, POINT_Y_COLUMN, TEMP_BOUNDS_COLUMN, TEMP_POINT_COORDS_COLUMN, TEMP_XMAX_COLUMN,
  TEMP_XMIN_COLUMN, TEMP_YMAX_COLUMN, TEMP_YMIN_COLUMN,
};
use crate::optimized::{ClusteringFamily, OptimizedGeometry};
use crate::output::{ReprojectionSpec, transformed_bounds_expr, transformed_point_coords_expr};

/// Resolves one selected dataset extent in the output coordinate reference system.
pub(super) struct TargetExtentResolver<'a> {
  input: &'a dyn InputSource,
  input_dataframe: DataFrame,
  row_range: RowRange,
}

impl<'a> TargetExtentResolver<'a> {
  /// Construct target-extent resolution for one prepared input selection.
  pub(super) fn new(
    input: &'a dyn InputSource,
    input_dataframe: DataFrame,
    row_range: RowRange,
  ) -> Self {
    Self {
      input,
      input_dataframe,
      row_range,
    }
  }

  /// Resolve the selected-row extent in the output coordinate reference system.
  pub(super) async fn resolve(
    self,
    source: &ResolvedGeoParquetSource,
    geometry: &OptimizedGeometry,
    reprojection: &ReprojectionSpec,
  ) -> Result<Extent2D> {
    let source_metadata = self.input.source_metadata()?;
    let metadata_fast_path = self.row_range.is_full()
      && !reprojection.requires_reprojection()
      && source_metadata
        .geometry
        .as_ref()
        .filter(|metadata| metadata.column == geometry.geometry_spec.column)
        .and_then(|metadata| metadata.bbox)
        .is_some();
    let target_extent = if metadata_fast_path {
      source.source_extent
    } else {
      let aggregate_dataframe =
        target_extent_aggregate(self.input_dataframe, geometry, reprojection)?;
      let batches = aggregate_dataframe.collect().await?;
      extract_target_extent(&batches)?
    };
    Ok(target_extent)
  }
}

fn target_extent_aggregate(
  dataframe: DataFrame,
  geometry: &OptimizedGeometry,
  reprojection: &ReprojectionSpec,
) -> Result<DataFrame> {
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

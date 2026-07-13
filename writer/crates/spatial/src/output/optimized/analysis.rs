//! Resolves optimized geometry analysis from metadata or aggregate execution.

use anyhow::{Context, Result, bail};
use arrow_array::{Array, Float64Array, RecordBatch};
use datafusion::functions::core::expr_ext::FieldAccessor;
use datafusion::functions_aggregate::expr_fn::{max, min};
use datafusion::logical_expr::{Expr, expr_fn::ident};
use indicatif::ProgressBar;

use crate::analysis::{
  DisplayGeometryType, DisplayJobAnalysis, Extent2D, GeometryFamily, SpatialReferenceInfo,
};
use crate::geometry::GeometrySpec;
use crate::metadata::source::SourceDatasetMetadata;
use crate::output::optimized::clustering::{bounds_expr, point_expr};
use crate::output::optimized::multiscale::{
  POINT_X_COLUMN, POINT_Y_COLUMN, TEMP_BOUNDS_COLUMN, TEMP_POINT_COORDS_COLUMN, TEMP_XMAX_COLUMN,
  TEMP_XMIN_COLUMN, TEMP_YMAX_COLUMN, TEMP_YMIN_COLUMN,
};
use crate::output::reprojection::{
  TransformSpec, transformed_bounds_expr, transformed_point_coords_expr,
};

use super::execution::collect_dataframe_with_metric_polling;

/// Analyze a source DataFrame by decoding geometry expressions and collecting aggregates.
pub(crate) async fn analyze_display_dataframe(
  dataframe: engine::DataFrame,
  geometry_spec: &GeometrySpec,
  source_metadata: &SourceDatasetMetadata,
  geometry_type: DisplayGeometryType,
  transform: Option<&TransformSpec>,
  output_spatial_reference: SpatialReferenceInfo,
  progress_bar: &ProgressBar,
  total_input_rows: u64,
  explain: bool,
) -> Result<DisplayJobAnalysis> {
  let aggregate_dataframe = match (geometry_type.family(), transform) {
    (GeometryFamily::Point, Some(transform)) => dataframe
      .with_column(
        TEMP_POINT_COORDS_COLUMN,
        transformed_point_coords_expr(&geometry_spec.column, transform),
      )?
      .select(vec![
        point_coords_field_expr("x").alias(POINT_X_COLUMN),
        point_coords_field_expr("y").alias(POINT_Y_COLUMN),
      ])?
      .aggregate(
        vec![],
        extent_aggregate_expressions(
          POINT_X_COLUMN,
          POINT_Y_COLUMN,
          POINT_X_COLUMN,
          POINT_Y_COLUMN,
        ),
      )?,
    (GeometryFamily::Point, None) => dataframe
      .with_column(TEMP_POINT_COORDS_COLUMN, point_expr(&geometry_spec.column))?
      .select(vec![
        point_coords_field_expr("x").alias(POINT_X_COLUMN),
        point_coords_field_expr("y").alias(POINT_Y_COLUMN),
      ])?
      .aggregate(
        vec![],
        extent_aggregate_expressions(
          POINT_X_COLUMN,
          POINT_Y_COLUMN,
          POINT_X_COLUMN,
          POINT_Y_COLUMN,
        ),
      )?,
    (GeometryFamily::NonPoint, Some(transform)) => dataframe
      .with_column(
        TEMP_BOUNDS_COLUMN,
        transformed_bounds_expr(&geometry_spec.column, geometry_type, transform),
      )?
      .select(vec![
        bounds_field_expr("xmin").alias(TEMP_XMIN_COLUMN),
        bounds_field_expr("ymin").alias(TEMP_YMIN_COLUMN),
        bounds_field_expr("xmax").alias(TEMP_XMAX_COLUMN),
        bounds_field_expr("ymax").alias(TEMP_YMAX_COLUMN),
      ])?
      .aggregate(
        vec![],
        extent_aggregate_expressions(
          TEMP_XMIN_COLUMN,
          TEMP_YMIN_COLUMN,
          TEMP_XMAX_COLUMN,
          TEMP_YMAX_COLUMN,
        ),
      )?,
    (GeometryFamily::NonPoint, None) => dataframe
      .with_column(TEMP_BOUNDS_COLUMN, bounds_expr(&geometry_spec.column))?
      .select(vec![
        bounds_field_expr("xmin").alias(TEMP_XMIN_COLUMN),
        bounds_field_expr("ymin").alias(TEMP_YMIN_COLUMN),
        bounds_field_expr("xmax").alias(TEMP_XMAX_COLUMN),
        bounds_field_expr("ymax").alias(TEMP_YMAX_COLUMN),
      ])?
      .aggregate(
        vec![],
        extent_aggregate_expressions(
          TEMP_XMIN_COLUMN,
          TEMP_YMIN_COLUMN,
          TEMP_XMAX_COLUMN,
          TEMP_YMAX_COLUMN,
        ),
      )?,
  };
  finish_analysis(
    aggregate_dataframe,
    geometry_spec,
    source_metadata,
    geometry_type,
    output_spatial_reference,
    progress_bar,
    total_input_rows,
    explain,
  )
  .await
}

/// Analyze a helper DataFrame whose transformed coordinates or bounds already exist.
pub(crate) async fn analyze_helper_dataframe(
  dataframe: engine::DataFrame,
  geometry_spec: &GeometrySpec,
  source_metadata: &SourceDatasetMetadata,
  geometry_type: DisplayGeometryType,
  output_spatial_reference: SpatialReferenceInfo,
  progress_bar: &ProgressBar,
  total_input_rows: u64,
  explain: bool,
) -> Result<DisplayJobAnalysis> {
  let aggregate_expressions = match geometry_type.family() {
    GeometryFamily::Point => extent_aggregate_expressions(
      POINT_X_COLUMN,
      POINT_Y_COLUMN,
      POINT_X_COLUMN,
      POINT_Y_COLUMN,
    ),
    GeometryFamily::NonPoint => extent_aggregate_expressions(
      TEMP_XMIN_COLUMN,
      TEMP_YMIN_COLUMN,
      TEMP_XMAX_COLUMN,
      TEMP_YMAX_COLUMN,
    ),
  };
  finish_analysis(
    dataframe.aggregate(vec![], aggregate_expressions)?,
    geometry_spec,
    source_metadata,
    geometry_type,
    output_spatial_reference,
    progress_bar,
    total_input_rows,
    explain,
  )
  .await
}

/// Build analysis from complete metadata in the exact output coordinate space.
pub(crate) fn metadata_display_analysis(
  geometry_spec: &GeometrySpec,
  source_metadata: &SourceDatasetMetadata,
  geometry_type: DisplayGeometryType,
  spatial_reference: SpatialReferenceInfo,
  allow_fast_path: bool,
) -> Option<DisplayJobAnalysis> {
  if !allow_fast_path {
    return None;
  }
  let source_geometry = source_metadata
    .geometry
    .as_ref()
    .filter(|geometry| geometry.column == geometry_spec.column)?;
  Some(DisplayJobAnalysis {
    geometry_spec: geometry_spec.clone(),
    geometry_type,
    geometry_family: geometry_type.family(),
    full_extent: source_geometry.bbox?,
    spatial_reference,
    has_z: source_geometry.has_z,
    has_m: source_geometry.has_m,
  })
}

async fn finish_analysis(
  aggregate_dataframe: engine::DataFrame,
  geometry_spec: &GeometrySpec,
  source_metadata: &SourceDatasetMetadata,
  geometry_type: DisplayGeometryType,
  output_spatial_reference: SpatialReferenceInfo,
  progress_bar: &ProgressBar,
  total_input_rows: u64,
  explain: bool,
) -> Result<DisplayJobAnalysis> {
  let batches = collect_dataframe_with_metric_polling(
    aggregate_dataframe,
    progress_bar,
    total_input_rows,
    "Analyzing geometry",
    explain,
  )
  .await?;
  let full_extent = extract_extent_from_aggregate_batches(&batches)?;
  let (has_z, has_m) = source_metadata
    .geometry
    .as_ref()
    .filter(|geometry| geometry.column == geometry_spec.column)
    .map(|geometry| (geometry.has_z, geometry.has_m))
    .unwrap_or((false, false));
  Ok(DisplayJobAnalysis {
    geometry_spec: geometry_spec.clone(),
    geometry_type,
    geometry_family: geometry_type.family(),
    full_extent,
    spatial_reference: output_spatial_reference,
    has_z,
    has_m,
  })
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

fn point_coords_field_expr(field: &str) -> Expr {
  ident(TEMP_POINT_COORDS_COLUMN).field(field)
}

fn bounds_field_expr(field: &str) -> Expr {
  ident(TEMP_BOUNDS_COLUMN).field(field)
}

fn extract_extent_from_aggregate_batches(batches: &[RecordBatch]) -> Result<Extent2D> {
  let Some(batch) = batches.first() else {
    bail!("unable to determine dataset full extent");
  };
  if batch.num_rows() == 0 {
    bail!("unable to determine dataset full extent");
  }
  Ok(Extent2D {
    xmin: extract_float_aggregate_value(batch, 0, "xmin")?,
    ymin: extract_float_aggregate_value(batch, 1, "ymin")?,
    xmax: extract_float_aggregate_value(batch, 2, "xmax")?,
    ymax: extract_float_aggregate_value(batch, 3, "ymax")?,
  })
}

fn extract_float_aggregate_value(
  batch: &RecordBatch,
  column_index: usize,
  label: &str,
) -> Result<f64> {
  let values = batch
    .column(column_index)
    .as_any()
    .downcast_ref::<Float64Array>()
    .with_context(|| format!("analysis aggregate column '{label}' was not Float64"))?;
  if values.is_null(0) {
    bail!("unable to determine dataset full extent");
  }
  Ok(values.value(0))
}

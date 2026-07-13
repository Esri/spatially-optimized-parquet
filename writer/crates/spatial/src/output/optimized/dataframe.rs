//! Builds helper projections, final projections, and spatially ordered DataFrames.

use crate::analysis::{DisplayGeometryType, DisplayJobAnalysis, GeometryFamily};
use crate::geometry::GeometrySpec;
use crate::input::materialized::input_dataframe_for_job;
use crate::input::{InputSource, RowRange};
use crate::output::optimized::multiscale::{
  DISPLAY_COLUMN, GeometryEncoding, POINT_X_COLUMN, POINT_Y_COLUMN, POINT_Z_CODE_COLUMN,
  TEMP_REPROJECTED_GEOMETRY_COLUMN, TEMP_XMAX_COLUMN, TEMP_XMIN_COLUMN, TEMP_XZ_CODE_COLUMN,
  TEMP_YMAX_COLUMN, TEMP_YMIN_COLUMN,
};
use crate::progress::finish_row_bar;
use crate::reprojection::TransformSpec;
use crate::udf::{
  bounds_xmax_expr, bounds_xmin_expr, bounds_ymax_expr, bounds_ymin_expr, feature_bbox_expr,
  non_point_geodisplay_expr, non_point_xzcode_from_bounds_expr, point_x_expr, point_y_expr,
  point_zcode_from_xy_expr, reproject_geometry_expr,
};
use anyhow::Result;
use datafusion::logical_expr::Expr;
use datafusion::logical_expr::expr_fn::ident;

use super::partitioning::compute_range_partition_boundaries;
use super::plan::{build_range_partition_expr, partition_column_name, sort_column_name, sort_expr};

/// Carries source and runtime state needed to construct ordered DataFrames.
pub(crate) struct OrderedDataframeRequest<'a> {
  pub(crate) input: &'a dyn InputSource,
  pub(crate) session: &'a engine::SessionContext,
  pub(crate) source_schema: &'a arrow_schema::Schema,
  pub(crate) row_range: RowRange,
  pub(crate) materialized_batches: Option<&'a [arrow_array::RecordBatch]>,
  pub(crate) output_parts: usize,
  pub(crate) total_input_rows: u64,
  pub(crate) progress: bool,
  pub(crate) explain: bool,
  pub(crate) geometry_spec: &'a GeometrySpec,
  pub(crate) geometry_type: DisplayGeometryType,
  pub(crate) transform: Option<&'a TransformSpec>,
}

/// Build either a globally sorted DataFrame or a range-partitioned multi-file DataFrame.
pub(crate) async fn prepare_spatially_ordered_dataframe(
  request: &OrderedDataframeRequest<'_>,
  analysis: &DisplayJobAnalysis,
) -> Result<engine::DataFrame> {
  if request.output_parts > 1 {
    let partition_column = partition_column_name(analysis);
    let range_bar = crate::progress::row_bar(
      request.progress,
      "Computing partition ranges",
      request.total_input_rows,
    );
    let range_source = add_sort_columns_dataframe(
      build_narrow_helper_projection_dataframe(
        input_dataframe_for_job(
          request.input,
          request.session,
          request.row_range,
          request.materialized_batches,
        )
        .await?,
        request.geometry_spec,
        request.geometry_type,
        request.transform,
      )?,
      analysis,
    )?;
    let boundaries = compute_range_partition_boundaries(
      range_source,
      sort_column_name(analysis),
      request.output_parts,
      &range_bar,
      request.total_input_rows,
      request.explain,
    )
    .await?;
    finish_row_bar(
      &range_bar,
      request.total_input_rows,
      "Computed partition ranges".to_string(),
    );
    return Ok(
      build_helper_projection_dataframe(
        input_dataframe_for_job(
          request.input,
          request.session,
          request.row_range,
          request.materialized_batches,
        )
        .await?,
        request.source_schema,
        analysis,
        request.transform,
      )?
      .with_column(
        partition_column,
        build_range_partition_expr(
          sort_column_name(analysis),
          boundaries.min_value,
          &boundaries.boundaries,
        )?,
      )?,
    );
  }

  build_helper_projection_dataframe(
    input_dataframe_for_job(
      request.input,
      request.session,
      request.row_range,
      request.materialized_batches,
    )
    .await?,
    request.source_schema,
    analysis,
    request.transform,
  )?
  .sort(vec![sort_expr(analysis)])
  .map_err(Into::into)
}

/// Build a geometry-only helper projection for analysis and range estimation.
pub(crate) fn build_narrow_helper_projection_dataframe(
  dataframe: engine::DataFrame,
  geometry_spec: &GeometrySpec,
  geometry_type: DisplayGeometryType,
  transform: Option<&TransformSpec>,
) -> Result<engine::DataFrame> {
  let projected = dataframe.select(vec![ident(&geometry_spec.column)])?;
  add_geometry_helper_columns_dataframe(projected, geometry_spec, geometry_type, transform)
}

/// Build the final public projection and remove temporary planning columns.
pub(crate) fn build_final_projection_expressions(
  source_schema: &arrow_schema::Schema,
  analysis: &DisplayJobAnalysis,
  encodings: &[GeometryEncoding],
  partition_column: Option<&str>,
  retained_sort_column: Option<&str>,
  projected_geometry_column: Option<&str>,
  covering: bool,
) -> Vec<Expr> {
  let mut expressions = source_schema
    .fields()
    .iter()
    .filter(|field| !is_generated_display_output_column(field.name(), analysis.geometry_family))
    .map(|field| {
      if field.name() == &analysis.geometry_spec.column
        && let Some(projected_geometry_column) = projected_geometry_column
      {
        return ident(projected_geometry_column).alias(field.name());
      }
      ident(field.name())
    })
    .collect::<Vec<_>>();
  if covering {
    let geometry_column = projected_geometry_column.unwrap_or(&analysis.geometry_spec.column);
    match analysis.geometry_family {
      GeometryFamily::Point => expressions.push(feature_bbox_expr(
        geometry_column,
        POINT_X_COLUMN,
        POINT_Y_COLUMN,
        POINT_X_COLUMN,
        POINT_Y_COLUMN,
      )),
      GeometryFamily::NonPoint => expressions.push(feature_bbox_expr(
        geometry_column,
        TEMP_XMIN_COLUMN,
        TEMP_YMIN_COLUMN,
        TEMP_XMAX_COLUMN,
        TEMP_YMAX_COLUMN,
      )),
    }
  }
  match analysis.geometry_family {
    GeometryFamily::Point => {
      expressions.push(ident(POINT_Z_CODE_COLUMN));
      expressions.push(ident(POINT_X_COLUMN));
      expressions.push(ident(POINT_Y_COLUMN));
    }
    GeometryFamily::NonPoint => expressions.push(non_point_geodisplay_expr(
      projected_geometry_column.unwrap_or(&analysis.geometry_spec.column),
      analysis.geometry_type,
      encodings,
    )),
  }
  if let Some(partition_column) = partition_column {
    expressions.push(ident(partition_column));
  }
  if let Some(retained_sort_column) = retained_sort_column {
    expressions.push(ident(retained_sort_column));
  }
  expressions
}

fn build_base_helper_projection_dataframe(
  dataframe: engine::DataFrame,
  source_schema: &arrow_schema::Schema,
  geometry_spec: &GeometrySpec,
  geometry_type: DisplayGeometryType,
  transform: Option<&TransformSpec>,
) -> Result<engine::DataFrame> {
  let projected = dataframe.select(
    source_schema
      .fields()
      .iter()
      .map(|field| ident(field.name()))
      .collect::<Vec<_>>(),
  )?;
  add_geometry_helper_columns_dataframe(projected, geometry_spec, geometry_type, transform)
}

fn add_geometry_helper_columns_dataframe(
  mut projected: engine::DataFrame,
  geometry_spec: &GeometrySpec,
  geometry_type: DisplayGeometryType,
  transform: Option<&TransformSpec>,
) -> Result<engine::DataFrame> {
  let geometry_column = if let Some(transform) = transform {
    projected = projected.with_column(
      TEMP_REPROJECTED_GEOMETRY_COLUMN,
      reproject_geometry_expr(&geometry_spec.column, transform),
    )?;
    TEMP_REPROJECTED_GEOMETRY_COLUMN
  } else {
    &geometry_spec.column
  };
  match geometry_type.family() {
    GeometryFamily::Point => {
      projected = projected.with_column(POINT_X_COLUMN, point_x_expr(geometry_column))?;
      projected = projected.with_column(POINT_Y_COLUMN, point_y_expr(geometry_column))?;
    }
    GeometryFamily::NonPoint => {
      projected = projected.with_column(TEMP_XMIN_COLUMN, bounds_xmin_expr(geometry_column))?;
      projected = projected.with_column(TEMP_YMIN_COLUMN, bounds_ymin_expr(geometry_column))?;
      projected = projected.with_column(TEMP_XMAX_COLUMN, bounds_xmax_expr(geometry_column))?;
      projected = projected.with_column(TEMP_YMAX_COLUMN, bounds_ymax_expr(geometry_column))?;
    }
  }
  Ok(projected)
}

fn add_sort_columns_dataframe(
  dataframe: engine::DataFrame,
  analysis: &DisplayJobAnalysis,
) -> Result<engine::DataFrame> {
  match analysis.geometry_family {
    GeometryFamily::Point => Ok(dataframe.with_column(
      POINT_Z_CODE_COLUMN,
      point_zcode_from_xy_expr(POINT_X_COLUMN, POINT_Y_COLUMN, analysis.full_extent),
    )?),
    GeometryFamily::NonPoint => Ok(dataframe.with_column(
      TEMP_XZ_CODE_COLUMN,
      non_point_xzcode_from_bounds_expr(
        TEMP_XMIN_COLUMN,
        TEMP_YMIN_COLUMN,
        TEMP_XMAX_COLUMN,
        TEMP_YMAX_COLUMN,
        analysis.full_extent,
      ),
    )?),
  }
}

fn build_helper_projection_dataframe(
  dataframe: engine::DataFrame,
  source_schema: &arrow_schema::Schema,
  analysis: &DisplayJobAnalysis,
  transform: Option<&TransformSpec>,
) -> Result<engine::DataFrame> {
  let projected = build_base_helper_projection_dataframe(
    dataframe,
    source_schema,
    &analysis.geometry_spec,
    analysis.geometry_type,
    transform,
  )?;
  add_sort_columns_dataframe(projected, analysis)
}

fn is_generated_display_output_column(name: &str, geometry_family: GeometryFamily) -> bool {
  match geometry_family {
    GeometryFamily::Point => {
      matches!(name, POINT_Z_CODE_COLUMN | POINT_X_COLUMN | POINT_Y_COLUMN)
    }
    GeometryFamily::NonPoint => name == DISPLAY_COLUMN,
  }
}

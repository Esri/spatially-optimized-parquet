//! Creates the lazy spatially ordered projection for optimized output.

use crate::geoparquet::bbox_field_expr;
use crate::optimized::clustering::{
  ClusterRangeBoundaries, cluster_key_column, cluster_partition_column, cluster_sort_expr,
  non_point_xzcode_from_bounds_expr, point_expr, point_zcode_from_xy_expr,
};
use crate::optimized::multiscale::non_point_geodisplay_expr;
use crate::optimized::multiscale::{
  COVERING_BBOX_COLUMN, GEODISPLAY_COLUMN, POINT_X_COLUMN, POINT_Y_COLUMN, POINT_Z_CODE_COLUMN,
  TEMP_XZ_CODE_COLUMN,
};
use crate::optimized::{ClusteringFamily, OptimizedGeometry, ResolvedOptimization};
use anyhow::Result;
use datafusion::dataframe::DataFrame;
use datafusion::functions::core::expr_ext::FieldAccessor;
use datafusion::logical_expr::Expr;
use datafusion::logical_expr::expr_fn::ident;

/// Create the globally sorted lazy projection for single-file output.
pub(super) fn single_file_projection(
  context: &ResolvedOptimization,
  input_dataframe: DataFrame,
  source_schema: &arrow_schema::Schema,
  covering: bool,
) -> Result<DataFrame> {
  let retained_cluster_key_column = matches!(
    context.geometry().clustering_family,
    ClusteringFamily::NonPoint
  )
  .then_some(cluster_key_column(context.geometry().clustering_family));
  let ordered_dataframe = helper_projection(input_dataframe, source_schema, context)?.sort(
    vec![cluster_sort_expr(context.geometry().clustering_family)],
  )?;
  ordered_dataframe
    .select(output_projection_expressions(
      source_schema,
      context,
      None,
      retained_cluster_key_column,
      covering,
    ))
    .map_err(Into::into)
}

/// Create the cluster-key query consumed by partition-boundary analysis.
pub(super) fn partitioned_range_source(
  context: &ResolvedOptimization,
  input_dataframe: DataFrame,
) -> Result<DataFrame> {
  add_sort_columns_dataframe(
    narrow_helper_projection(input_dataframe, context.geometry())?,
    context,
  )
}

/// Create the range-key lazy projection consumed by partitioned physical output.
pub(super) fn partitioned_projection(
  context: &ResolvedOptimization,
  input_dataframe: DataFrame,
  source_schema: &arrow_schema::Schema,
  boundaries: &ClusterRangeBoundaries,
  covering: bool,
) -> Result<DataFrame> {
  let partition_column = cluster_partition_column(context.geometry().clustering_family);
  let dataframe = helper_projection(input_dataframe, source_schema, context)?.with_column(
    partition_column,
    boundaries.partition_expr(cluster_key_column(context.geometry().clustering_family))?,
  )?;
  let retained_cluster_key_column = matches!(
    context.geometry().clustering_family,
    ClusteringFamily::NonPoint
  )
  .then_some(cluster_key_column(context.geometry().clustering_family));
  dataframe
    .select(output_projection_expressions(
      source_schema,
      context,
      Some(partition_column),
      retained_cluster_key_column,
      covering,
    ))
    .map_err(Into::into)
}

fn narrow_helper_projection(
  dataframe: DataFrame,
  geometry: &OptimizedGeometry,
) -> Result<DataFrame> {
  let projected = dataframe.select(vec![
    ident(&geometry.geometry_spec.column),
    ident(COVERING_BBOX_COLUMN),
  ])?;
  add_geometry_helper_columns_dataframe(projected, geometry, false)
}

fn output_projection_expressions(
  source_schema: &arrow_schema::Schema,
  context: &ResolvedOptimization,
  partition_column: Option<&str>,
  retained_cluster_key_column: Option<&str>,
  covering: bool,
) -> Vec<Expr> {
  let mut expressions = source_schema
    .fields()
    .iter()
    .filter(|field| {
      field.name() != COVERING_BBOX_COLUMN
        && !is_generated_optimized_output_column(field.name(), context.geometry().clustering_family)
    })
    .map(|field| ident(field.name()))
    .collect::<Vec<_>>();
  if covering {
    expressions.push(ident(COVERING_BBOX_COLUMN));
  }
  match context.geometry().clustering_family {
    ClusteringFamily::Point => {
      expressions.push(ident(POINT_Z_CODE_COLUMN));
      expressions.push(ident(POINT_X_COLUMN));
      expressions.push(ident(POINT_Y_COLUMN));
    }
    ClusteringFamily::NonPoint => expressions.push(non_point_geodisplay_expr(
      &context.geometry().geometry_spec.column,
      context.geometry().geometry_type,
      context.encodings(),
    )),
  }
  if let Some(partition_column) = partition_column {
    expressions.push(ident(partition_column));
  }
  if let Some(retained_cluster_key_column) = retained_cluster_key_column {
    expressions.push(ident(retained_cluster_key_column));
  }
  expressions
}

fn base_helper_projection(
  dataframe: DataFrame,
  source_schema: &arrow_schema::Schema,
  geometry: &OptimizedGeometry,
  point_optimization_reused: bool,
) -> Result<DataFrame> {
  let projected = dataframe.select(
    source_schema
      .fields()
      .iter()
      .filter(|field| field.name() != COVERING_BBOX_COLUMN)
      .map(|field| ident(field.name()))
      .chain(std::iter::once(ident(COVERING_BBOX_COLUMN)))
      .collect::<Vec<_>>(),
  )?;
  add_geometry_helper_columns_dataframe(projected, geometry, point_optimization_reused)
}

fn add_geometry_helper_columns_dataframe(
  mut projected: DataFrame,
  geometry: &OptimizedGeometry,
  point_optimization_reused: bool,
) -> Result<DataFrame> {
  match geometry.clustering_family {
    ClusteringFamily::Point if !point_optimization_reused => {
      let point = point_expr(&geometry.geometry_spec.column);
      projected = projected.with_column(POINT_X_COLUMN, point.clone().field("x"))?;
      projected = projected.with_column(POINT_Y_COLUMN, point.field("y"))?;
    }
    ClusteringFamily::Point | ClusteringFamily::NonPoint => {}
  }
  Ok(projected)
}

fn add_sort_columns_dataframe(
  dataframe: DataFrame,
  context: &ResolvedOptimization,
) -> Result<DataFrame> {
  match context.geometry().clustering_family {
    ClusteringFamily::Point if !context.point_optimization_reused() => Ok(dataframe.with_column(
      POINT_Z_CODE_COLUMN,
      point_zcode_from_xy_expr(
        bbox_field_expr("xmin"),
        bbox_field_expr("ymin"),
        context.target_extent(),
      ),
    )?),
    ClusteringFamily::Point => Ok(dataframe),
    ClusteringFamily::NonPoint => Ok(dataframe.with_column(
      TEMP_XZ_CODE_COLUMN,
      non_point_xzcode_from_bounds_expr(
        bbox_field_expr("xmin"),
        bbox_field_expr("ymin"),
        bbox_field_expr("xmax"),
        bbox_field_expr("ymax"),
        context.target_extent(),
      ),
    )?),
  }
}

fn helper_projection(
  dataframe: DataFrame,
  source_schema: &arrow_schema::Schema,
  context: &ResolvedOptimization,
) -> Result<DataFrame> {
  let projected = base_helper_projection(
    dataframe,
    source_schema,
    context.geometry(),
    context.point_optimization_reused(),
  )?;
  add_sort_columns_dataframe(projected, context)
}

fn is_generated_optimized_output_column(name: &str, clustering_family: ClusteringFamily) -> bool {
  match clustering_family {
    ClusteringFamily::Point => {
      matches!(name, POINT_Z_CODE_COLUMN | POINT_X_COLUMN | POINT_Y_COLUMN)
    }
    ClusteringFamily::NonPoint => name == GEODISPLAY_COLUMN,
  }
}

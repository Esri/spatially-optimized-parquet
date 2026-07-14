//! Creates the lazy spatially ordered projection for optimized output.

use crate::geoparquet::feature_bbox_expr;
use crate::optimized::clustering::{
  ClusterRangeBoundaries, bounds_expr, cluster_key_column, cluster_partition_column,
  cluster_sort_expr, non_point_xzcode_from_bounds_expr, point_expr, point_zcode_from_xy_expr,
};
use crate::optimized::multiscale::non_point_geodisplay_expr;
use crate::optimized::multiscale::{
  GEODISPLAY_COLUMN, POINT_X_COLUMN, POINT_Y_COLUMN, POINT_Z_CODE_COLUMN, TEMP_BOUNDS_COLUMN,
  TEMP_POINT_COORDS_COLUMN, TEMP_REPROJECTED_GEOMETRY_COLUMN, TEMP_XMAX_COLUMN, TEMP_XMIN_COLUMN,
  TEMP_XZ_CODE_COLUMN, TEMP_YMAX_COLUMN, TEMP_YMIN_COLUMN,
};
use crate::optimized::{ClusteringFamily, OptimizedGeometry, ResolvedOptimization};
use crate::output::{CoordinateTransformSpec, reproject_geometry_expr};
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
  let ordered_dataframe = helper_projection(input_dataframe, source_schema, context)?.sort(
    vec![cluster_sort_expr(context.geometry().clustering_family)],
  )?;
  ordered_dataframe
    .select(output_projection_expressions(
      source_schema,
      context,
      None,
      None,
      context
        .reprojection()
        .requires_reprojection()
        .then_some(TEMP_REPROJECTED_GEOMETRY_COLUMN),
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
    narrow_helper_projection(
      input_dataframe,
      context.geometry(),
      context.reprojection().transform(),
    )?,
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
      context
        .reprojection()
        .requires_reprojection()
        .then_some(TEMP_REPROJECTED_GEOMETRY_COLUMN),
      covering,
    ))
    .map_err(Into::into)
}

fn narrow_helper_projection(
  dataframe: DataFrame,
  geometry: &OptimizedGeometry,
  transform: Option<&CoordinateTransformSpec>,
) -> Result<DataFrame> {
  let projected = dataframe.select(vec![ident(&geometry.geometry_spec.column)])?;
  add_geometry_helper_columns_dataframe(projected, geometry, transform)
}

fn output_projection_expressions(
  source_schema: &arrow_schema::Schema,
  context: &ResolvedOptimization,
  partition_column: Option<&str>,
  retained_cluster_key_column: Option<&str>,
  projected_geometry_column: Option<&str>,
  covering: bool,
) -> Vec<Expr> {
  let mut expressions = source_schema
    .fields()
    .iter()
    .filter(|field| {
      !is_generated_optimized_output_column(field.name(), context.geometry().clustering_family)
    })
    .map(|field| {
      if field.name() == &context.geometry().geometry_spec.column
        && let Some(projected_geometry_column) = projected_geometry_column
      {
        return ident(projected_geometry_column).alias(field.name());
      }
      ident(field.name())
    })
    .collect::<Vec<_>>();
  if covering {
    let geometry_column =
      projected_geometry_column.unwrap_or(&context.geometry().geometry_spec.column);
    match context.geometry().clustering_family {
      ClusteringFamily::Point => expressions.push(feature_bbox_expr(
        geometry_column,
        POINT_X_COLUMN,
        POINT_Y_COLUMN,
        POINT_X_COLUMN,
        POINT_Y_COLUMN,
      )),
      ClusteringFamily::NonPoint => expressions.push(feature_bbox_expr(
        geometry_column,
        TEMP_XMIN_COLUMN,
        TEMP_YMIN_COLUMN,
        TEMP_XMAX_COLUMN,
        TEMP_YMAX_COLUMN,
      )),
    }
  }
  match context.geometry().clustering_family {
    ClusteringFamily::Point => {
      expressions.push(ident(POINT_Z_CODE_COLUMN));
      expressions.push(ident(POINT_X_COLUMN));
      expressions.push(ident(POINT_Y_COLUMN));
    }
    ClusteringFamily::NonPoint => expressions.push(non_point_geodisplay_expr(
      projected_geometry_column.unwrap_or(&context.geometry().geometry_spec.column),
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
  transform: Option<&CoordinateTransformSpec>,
) -> Result<DataFrame> {
  let projected = dataframe.select(
    source_schema
      .fields()
      .iter()
      .map(|field| ident(field.name()))
      .collect::<Vec<_>>(),
  )?;
  add_geometry_helper_columns_dataframe(projected, geometry, transform)
}

fn add_geometry_helper_columns_dataframe(
  mut projected: DataFrame,
  geometry: &OptimizedGeometry,
  transform: Option<&CoordinateTransformSpec>,
) -> Result<DataFrame> {
  let geometry_column = if let Some(transform) = transform {
    projected = projected.with_column(
      TEMP_REPROJECTED_GEOMETRY_COLUMN,
      reproject_geometry_expr(&geometry.geometry_spec.column, transform),
    )?;
    TEMP_REPROJECTED_GEOMETRY_COLUMN
  } else {
    &geometry.geometry_spec.column
  };
  match geometry.clustering_family {
    ClusteringFamily::Point => {
      projected = projected.with_column(TEMP_POINT_COORDS_COLUMN, point_expr(geometry_column))?;
      projected =
        projected.with_column(POINT_X_COLUMN, ident(TEMP_POINT_COORDS_COLUMN).field("x"))?;
      projected =
        projected.with_column(POINT_Y_COLUMN, ident(TEMP_POINT_COORDS_COLUMN).field("y"))?;
    }
    ClusteringFamily::NonPoint => {
      projected = projected.with_column(TEMP_BOUNDS_COLUMN, bounds_expr(geometry_column))?;
      projected =
        projected.with_column(TEMP_XMIN_COLUMN, ident(TEMP_BOUNDS_COLUMN).field("xmin"))?;
      projected =
        projected.with_column(TEMP_YMIN_COLUMN, ident(TEMP_BOUNDS_COLUMN).field("ymin"))?;
      projected =
        projected.with_column(TEMP_XMAX_COLUMN, ident(TEMP_BOUNDS_COLUMN).field("xmax"))?;
      projected =
        projected.with_column(TEMP_YMAX_COLUMN, ident(TEMP_BOUNDS_COLUMN).field("ymax"))?;
    }
  }
  Ok(projected)
}

fn add_sort_columns_dataframe(
  dataframe: DataFrame,
  context: &ResolvedOptimization,
) -> Result<DataFrame> {
  match context.geometry().clustering_family {
    ClusteringFamily::Point => Ok(dataframe.with_column(
      POINT_Z_CODE_COLUMN,
      point_zcode_from_xy_expr(POINT_X_COLUMN, POINT_Y_COLUMN, context.target_extent()),
    )?),
    ClusteringFamily::NonPoint => Ok(dataframe.with_column(
      TEMP_XZ_CODE_COLUMN,
      non_point_xzcode_from_bounds_expr(
        TEMP_XMIN_COLUMN,
        TEMP_YMIN_COLUMN,
        TEMP_XMAX_COLUMN,
        TEMP_YMAX_COLUMN,
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
    context.reprojection().transform(),
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

//! Builds typed DataFusion expressions for geometry processing.

use datafusion::logical_expr::Expr;
use datafusion::prelude::{col, lit};

use crate::analysis::{DisplayGeometryType, Extent2D};
use crate::output::optimized::multiscale::{
  COVERING_BBOX_COLUMN, DISPLAY_COLUMN, GeometryEncoding, POINT_X_COLUMN, POINT_Y_COLUMN,
  POINT_Z_CODE_COLUMN, TEMP_XMAX_COLUMN, TEMP_XMIN_COLUMN, TEMP_XZ_CODE_COLUMN, TEMP_YMAX_COLUMN,
  TEMP_YMIN_COLUMN,
};
use crate::reprojection::TransformSpec;

use super::clustering::{
  bounds_xmax_udf, bounds_xmin_udf, bounds_ymax_udf, bounds_ymin_udf,
  non_point_xzcode_from_bounds_udf, non_point_xzcode_udf, point_x_udf, point_y_udf,
  point_zcode_from_xy_udf, point_zcode_udf,
};
use super::multiscale::non_point_geodisplay_udf;
use super::reprojection::{
  feature_bbox_udf, reproject_geometry_udf, transformed_bounds_udf, transformed_point_coords_udf,
};

/// Build a point Z-order expression from a WKB geometry column.
pub fn point_zcode_expr(geometry_column: &str, full_extent: Extent2D) -> Expr {
  point_zcode_udf()
    .call(vec![
      col(geometry_column),
      lit(full_extent.xmin),
      lit(full_extent.ymin),
      lit(full_extent.xmax),
      lit(full_extent.ymax),
    ])
    .alias(POINT_Z_CODE_COLUMN)
}

/// Build an expression that extracts point x coordinates from WKB.
pub fn point_x_expr(geometry_column: &str) -> Expr {
  point_x_udf()
    .call(vec![col(geometry_column)])
    .alias(POINT_X_COLUMN)
}

/// Build an expression that extracts point y coordinates from WKB.
pub fn point_y_expr(geometry_column: &str) -> Expr {
  point_y_udf()
    .call(vec![col(geometry_column)])
    .alias(POINT_Y_COLUMN)
}

/// Build a struct expression containing target-CRS point coordinates.
pub fn transformed_point_coords_expr(geometry_column: &str, transform: &TransformSpec) -> Expr {
  transformed_point_coords_udf(transform.clone()).call(vec![col(geometry_column)])
}

/// Build a point Z-order expression from precomputed x/y columns.
pub fn point_zcode_from_xy_expr(x_column: &str, y_column: &str, full_extent: Extent2D) -> Expr {
  point_zcode_from_xy_udf()
    .call(vec![
      col(x_column),
      col(y_column),
      lit(full_extent.xmin),
      lit(full_extent.ymin),
      lit(full_extent.xmax),
      lit(full_extent.ymax),
    ])
    .alias(POINT_Z_CODE_COLUMN)
}

/// Build a non-point XZ-order expression by decoding WKB bounds.
pub fn non_point_xzcode_expr(geometry_column: &str, full_extent: Extent2D) -> Expr {
  non_point_xzcode_udf()
    .call(vec![
      col(geometry_column),
      lit(full_extent.xmin),
      lit(full_extent.ymin),
      lit(full_extent.xmax),
      lit(full_extent.ymax),
    ])
    .alias(TEMP_XZ_CODE_COLUMN)
}

/// Build the non-point geodisplay struct containing code, bounds, and encoded LOD columns.
pub fn non_point_geodisplay_expr(
  geometry_column: &str,
  geometry_type: DisplayGeometryType,
  encodings: &[GeometryEncoding],
) -> Expr {
  non_point_geodisplay_udf(geometry_type, encodings.to_vec())
    .call(vec![
      col(geometry_column),
      col(TEMP_XZ_CODE_COLUMN),
      col(TEMP_XMIN_COLUMN),
      col(TEMP_YMIN_COLUMN),
      col(TEMP_XMAX_COLUMN),
      col(TEMP_YMAX_COLUMN),
    ])
    .alias(DISPLAY_COLUMN)
}

/// Build an expression that extracts geometry minimum x.
pub fn bounds_xmin_expr(geometry_column: &str) -> Expr {
  bounds_xmin_udf()
    .call(vec![col(geometry_column)])
    .alias(TEMP_XMIN_COLUMN)
}

/// Build an expression that extracts geometry minimum y.
pub fn bounds_ymin_expr(geometry_column: &str) -> Expr {
  bounds_ymin_udf()
    .call(vec![col(geometry_column)])
    .alias(TEMP_YMIN_COLUMN)
}

/// Build an expression that extracts geometry maximum x.
pub fn bounds_xmax_expr(geometry_column: &str) -> Expr {
  bounds_xmax_udf()
    .call(vec![col(geometry_column)])
    .alias(TEMP_XMAX_COLUMN)
}

/// Build an expression that extracts geometry maximum y.
pub fn bounds_ymax_expr(geometry_column: &str) -> Expr {
  bounds_ymax_udf()
    .call(vec![col(geometry_column)])
    .alias(TEMP_YMAX_COLUMN)
}

/// Build a struct expression containing geometry bounds in the target CRS.
pub fn transformed_bounds_expr(
  geometry_column: &str,
  geometry_type: DisplayGeometryType,
  transform: &TransformSpec,
) -> Expr {
  transformed_bounds_udf(transform.clone(), geometry_type).call(vec![col(geometry_column)])
}

/// Build a non-point XZ-order expression from precomputed bound columns.
pub fn non_point_xzcode_from_bounds_expr(
  xmin_column: &str,
  ymin_column: &str,
  xmax_column: &str,
  ymax_column: &str,
  full_extent: Extent2D,
) -> Expr {
  non_point_xzcode_from_bounds_udf()
    .call(vec![
      col(xmin_column),
      col(ymin_column),
      col(xmax_column),
      col(ymax_column),
      lit(full_extent.xmin),
      lit(full_extent.ymin),
      lit(full_extent.xmax),
      lit(full_extent.ymax),
    ])
    .alias(TEMP_XZ_CODE_COLUMN)
}

/// Build the GeoParquet covering bbox struct while preserving geometry nullability.
pub fn feature_bbox_expr(
  geometry_column: &str,
  xmin_column: &str,
  ymin_column: &str,
  xmax_column: &str,
  ymax_column: &str,
) -> Expr {
  feature_bbox_udf()
    .call(vec![
      col(geometry_column),
      col(xmin_column),
      col(ymin_column),
      col(xmax_column),
      col(ymax_column),
    ])
    .alias(COVERING_BBOX_COLUMN)
}

/// Build an expression that reprojects one WKB geometry column.
pub fn reproject_geometry_expr(geometry_column: &str, transform: &TransformSpec) -> Expr {
  reproject_geometry_udf(transform.clone()).call(vec![col(geometry_column)])
}

//! Adds geometry helpers and spatial cluster keys to DataFusion dataframes.

use anyhow::Result;
use datafusion::dataframe::DataFrame;
use datafusion::functions::core::expr_ext::FieldAccessor;

use crate::geometry::Extent2D;
use crate::geoparquet::bbox_field_expr;
use crate::optimized::multiscale::{
  POINT_M_COLUMN, POINT_X_COLUMN, POINT_Y_COLUMN, POINT_Z_CODE_COLUMN, POINT_Z_COLUMN,
  TEMP_XZ_CODE_COLUMN,
};

use super::{
  complex_geometry_xzcode_from_bounds_expr, point_geometry_expr_with_dimensions,
  point_geometry_zcode_from_xy_expr,
};
use crate::optimized::{ClusteringFamily, GeometryInfo};

/// Add geometry helpers and the canonical spatial cluster key to one dataframe.
pub(crate) fn clustering_dataframe(
  dataframe: DataFrame,
  geometry: &GeometryInfo,
  target_extent: Extent2D,
) -> Result<DataFrame> {
  let dataframe = add_point_columns(dataframe, geometry)?;
  add_cluster_key_column(dataframe, geometry.clustering_family, target_extent)
}

fn add_point_columns(mut dataframe: DataFrame, geometry: &GeometryInfo) -> Result<DataFrame> {
  if geometry.clustering_family != ClusteringFamily::PointGeometry {
    return Ok(dataframe);
  }
  let point = point_geometry_expr_with_dimensions(
    &geometry.geometry_spec.column,
    geometry.has_z,
    geometry.has_m,
  );
  dataframe = dataframe.with_column(POINT_X_COLUMN, point.clone().field("x"))?;
  dataframe = dataframe.with_column(POINT_Y_COLUMN, point.clone().field("y"))?;
  if geometry.has_z {
    dataframe = dataframe.with_column(POINT_Z_COLUMN, point.clone().field("z"))?;
  }
  if geometry.has_m {
    dataframe = dataframe.with_column(POINT_M_COLUMN, point.field("m"))?;
  }
  Ok(dataframe)
}

fn add_cluster_key_column(
  dataframe: DataFrame,
  clustering_family: ClusteringFamily,
  target_extent: Extent2D,
) -> Result<DataFrame> {
  match clustering_family {
    ClusteringFamily::PointGeometry => Ok(dataframe.with_column(
      POINT_Z_CODE_COLUMN,
      point_geometry_zcode_from_xy_expr(
        bbox_field_expr("xmin"),
        bbox_field_expr("ymin"),
        target_extent,
      ),
    )?),
    ClusteringFamily::ComplexGeometry => Ok(dataframe.with_column(
      TEMP_XZ_CODE_COLUMN,
      complex_geometry_xzcode_from_bounds_expr(
        bbox_field_expr("xmin"),
        bbox_field_expr("ymin"),
        bbox_field_expr("xmax"),
        bbox_field_expr("ymax"),
        target_extent,
      ),
    )?),
  }
}

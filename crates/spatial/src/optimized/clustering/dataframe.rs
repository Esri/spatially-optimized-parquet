//! Adds geometry helpers and spatial cluster keys to DataFusion dataframes.

use anyhow::Result;
use datafusion::dataframe::DataFrame;
use datafusion::functions::core::expr_ext::FieldAccessor;

use crate::geometry::Extent2D;
use crate::geoparquet::bbox_field_expr;
use crate::optimized::multiscale::{
  GEOKEY_COLUMN, POINT_M_COLUMN, POINT_X_COLUMN, POINT_Y_COLUMN, POINT_Z_COLUMN,
};

use super::{ComplexGeometryBoundsClusterKeyUdf, PointGeometryClusterKeyUdf, PointGeometryUdf};
use crate::optimized::{ClusteringFamily, GeometryInfo};

impl GeometryInfo {
  /// Add geometry helpers and the canonical spatial cluster key to one dataframe.
  pub(crate) fn clustering_dataframe(
    &self,
    dataframe: DataFrame,
    target_extent: Extent2D,
  ) -> Result<DataFrame> {
    let dataframe = self.add_point_columns(dataframe)?;
    self.add_cluster_key_column(dataframe, target_extent)
  }

  fn add_point_columns(&self, mut dataframe: DataFrame) -> Result<DataFrame> {
    if self.clustering_family != ClusteringFamily::PointGeometry {
      return Ok(dataframe);
    }
    let point =
      PointGeometryUdf::expression_with_dimensions(&self.geometry.column, self.has_z, self.has_m);
    dataframe = dataframe.with_column(POINT_X_COLUMN, point.clone().field("x"))?;
    dataframe = dataframe.with_column(POINT_Y_COLUMN, point.clone().field("y"))?;
    if self.has_z {
      dataframe = dataframe.with_column(POINT_Z_COLUMN, point.clone().field("z"))?;
    }
    if self.has_m {
      dataframe = dataframe.with_column(POINT_M_COLUMN, point.field("m"))?;
    }
    Ok(dataframe)
  }

  fn add_cluster_key_column(
    &self,
    dataframe: DataFrame,
    target_extent: Extent2D,
  ) -> Result<DataFrame> {
    match self.clustering_family {
      ClusteringFamily::PointGeometry => Ok(dataframe.with_column(
        GEOKEY_COLUMN,
        PointGeometryClusterKeyUdf::expression(
          bbox_field_expr("xmin"),
          bbox_field_expr("ymin"),
          target_extent,
        ),
      )?),
      ClusteringFamily::ComplexGeometry => Ok(dataframe.with_column(
        GEOKEY_COLUMN,
        ComplexGeometryBoundsClusterKeyUdf::expression(
          bbox_field_expr("xmin"),
          bbox_field_expr("ymin"),
          bbox_field_expr("xmax"),
          bbox_field_expr("ymax"),
          target_extent,
        ),
      )?),
    }
  }
}

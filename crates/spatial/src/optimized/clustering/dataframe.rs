// Copyright 2026 Esri
//
// Licensed under the Apache License Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Adds geometry helpers and spatial cluster keys to DataFusion dataframes.

use datafusion::dataframe::DataFrame;
use datafusion::error::Result;
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
    cluster_depth: u32,
  ) -> Result<DataFrame> {
    let dataframe = self.add_point_columns(dataframe)?;
    self.add_cluster_key_column(dataframe, target_extent, cluster_depth)
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
    cluster_depth: u32,
  ) -> Result<DataFrame> {
    match self.clustering_family {
      ClusteringFamily::PointGeometry => Ok(dataframe.with_column(
        GEOKEY_COLUMN,
        PointGeometryClusterKeyUdf::expression(
          bbox_field_expr("xmin"),
          bbox_field_expr("ymin"),
          target_extent,
          cluster_depth,
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
          cluster_depth,
        ),
      )?),
    }
  }
}

//! Selects common source, covering, and Geodisplay columns for optimized output.

use datafusion::logical_expr::Expr;
use datafusion::logical_expr::expr_fn::ident;

use crate::geoparquet::COVERING_BBOX_COLUMN;
use crate::optimized::multiscale::{
  GEODISPLAY_COLUMN, GEOKEY_COLUMN, GEOLOD_COLUMN, POINT_M_COLUMN, POINT_X_COLUMN, POINT_Y_COLUMN,
  POINT_Z_COLUMN, SOP_GEOMETRY_COLUMN,
};
use crate::pipeline::PipelineWarnings;

use super::multiscale::{GeolodUdf, SopGeometryUdf};
use super::{ClusteringFamily, OptimizedLayout};

impl OptimizedLayout {
  /// Select source columns, optional covering data, and generated Geodisplay output.
  pub(crate) fn output_expressions(
    &self,
    source_schema: &arrow_schema::Schema,
    covering: bool,
    warnings: PipelineWarnings,
  ) -> Vec<Expr> {
    let mut expressions = source_schema
      .fields()
      .iter()
      .filter(|field| {
        field.name() != COVERING_BBOX_COLUMN
          && !self
            .geometry()
            .clustering_family
            .is_generated_output_column(field.name())
      })
      .map(|field| ident(field.name()))
      .collect::<Vec<_>>();
    if covering {
      expressions.push(ident(COVERING_BBOX_COLUMN));
    }
    match self.geometry().clustering_family {
      ClusteringFamily::PointGeometry => {
        expressions.push(ident(GEOKEY_COLUMN));
        expressions.push(SopGeometryUdf::expression(
          self.geometry().has_z,
          self.geometry().has_m,
        ))
      }
      ClusteringFamily::ComplexGeometry => {
        expressions.push(ident(GEOKEY_COLUMN));
        if self.writes_lod_columns() {
          expressions.push(GeolodUdf::expression(
            &self.geometry().geometry.column,
            self.geometry().family,
            self.geometry().has_z,
            self.geometry().has_m,
            self.levels(),
            self.multiscale_encoding(),
            warnings,
          ));
        }
      }
    }
    expressions
  }
}

impl ClusteringFamily {
  fn is_generated_output_column(self, name: &str) -> bool {
    if matches!(name, GEOKEY_COLUMN | GEOLOD_COLUMN | GEODISPLAY_COLUMN) {
      return true;
    }

    match self {
      Self::PointGeometry => {
        matches!(
          name,
          SOP_GEOMETRY_COLUMN | POINT_X_COLUMN | POINT_Y_COLUMN | POINT_Z_COLUMN | POINT_M_COLUMN
        )
      }
      Self::ComplexGeometry => false,
    }
  }
}

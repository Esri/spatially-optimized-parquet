//! Selects common source, covering, and Geodisplay columns for optimized output.

use datafusion::logical_expr::Expr;
use datafusion::logical_expr::expr_fn::ident;

use crate::optimized::multiscale::{
  COVERING_BBOX_COLUMN, GEODISPLAY_COLUMN, POINT_M_COLUMN, POINT_X_COLUMN, POINT_Y_COLUMN,
  POINT_Z_CODE_COLUMN, POINT_Z_COLUMN,
};
use crate::pipeline::PipelineWarningStore;

use super::multiscale::{ComplexGeometryGeodisplayUdf, PointGeometryGeodisplayUdf};
use super::{ClusteringFamily, ResolvedOptimization};

impl ResolvedOptimization {
  /// Select source columns, optional covering data, and generated Geodisplay output.
  pub(super) fn output_expressions(
    &self,
    source_schema: &arrow_schema::Schema,
    covering: bool,
    warning_store: PipelineWarningStore,
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
      ClusteringFamily::PointGeometry => expressions.push(PointGeometryGeodisplayUdf::expression(
        self.geometry().has_z,
        self.geometry().has_m,
      )),
      ClusteringFamily::ComplexGeometry => {
        expressions.push(ComplexGeometryGeodisplayUdf::expression(
          &self.geometry().geometry.column,
          self.geometry().ty,
          self.geometry().has_z,
          self.geometry().has_m,
          self.levels(),
          self.multiscale_encoding(),
          warning_store,
        ))
      }
    }
    expressions
  }
}

impl ClusteringFamily {
  fn is_generated_output_column(self, name: &str) -> bool {
    match self {
      Self::PointGeometry => {
        matches!(
          name,
          GEODISPLAY_COLUMN
            | POINT_Z_CODE_COLUMN
            | POINT_X_COLUMN
            | POINT_Y_COLUMN
            | POINT_Z_COLUMN
            | POINT_M_COLUMN
        )
      }
      Self::ComplexGeometry => name == GEODISPLAY_COLUMN,
    }
  }
}

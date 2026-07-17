//! Selects common source, covering, and Geodisplay columns for optimized output.

use datafusion::logical_expr::Expr;
use datafusion::logical_expr::expr_fn::ident;

use crate::optimized::multiscale::{
  COVERING_BBOX_COLUMN, GEODISPLAY_COLUMN, POINT_M_COLUMN, POINT_X_COLUMN, POINT_Y_COLUMN,
  POINT_Z_CODE_COLUMN, POINT_Z_COLUMN,
};
use crate::optimized::multiscale::{
  complex_geometry_geodisplay_expr, point_geometry_geodisplay_expr,
};
use crate::pipeline::PipelineWarningStore;

use super::{ClusteringFamily, ResolvedOptimization};

/// Select source columns, optional covering data, and generated Geodisplay output.
pub(crate) fn output_expressions(
  source_schema: &arrow_schema::Schema,
  optimization: &ResolvedOptimization,
  covering: bool,
  warning_store: PipelineWarningStore,
) -> Vec<Expr> {
  let mut expressions = source_schema
    .fields()
    .iter()
    .filter(|field| {
      field.name() != COVERING_BBOX_COLUMN
        && !is_generated_optimized_output_column(
          field.name(),
          optimization.geometry().clustering_family,
        )
    })
    .map(|field| ident(field.name()))
    .collect::<Vec<_>>();
  if covering {
    expressions.push(ident(COVERING_BBOX_COLUMN));
  }
  match optimization.geometry().clustering_family {
    ClusteringFamily::PointGeometry => expressions.push(point_geometry_geodisplay_expr(
      optimization.geometry().has_z,
      optimization.geometry().has_m,
    )),
    ClusteringFamily::ComplexGeometry => expressions.push(complex_geometry_geodisplay_expr(
      &optimization.geometry().geometry.column,
      optimization.geometry().ty,
      optimization.geometry().has_z,
      optimization.geometry().has_m,
      optimization.levels(),
      optimization.multiscale_encoding(),
      warning_store,
    )),
  }
  expressions
}

fn is_generated_optimized_output_column(name: &str, clustering_family: ClusteringFamily) -> bool {
  match clustering_family {
    ClusteringFamily::PointGeometry => {
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
    ClusteringFamily::ComplexGeometry => name == GEODISPLAY_COLUMN,
  }
}

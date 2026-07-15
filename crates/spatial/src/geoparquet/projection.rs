//! Projects GeoParquet without optimized clustering columns or spatial sorting.

use anyhow::Result;
use datafusion::dataframe::DataFrame;
use datafusion::logical_expr::Expr;
use datafusion::logical_expr::expr_fn::ident;

use crate::optimized::COVERING_BBOX_COLUMN;

pub(super) fn plain_output_dataframe(
  dataframe: DataFrame,
  source_schema: &arrow_schema::Schema,
  geometry_column: &str,
  covering: bool,
) -> Result<DataFrame> {
  let mut expressions: Vec<Expr> = source_schema
    .fields()
    .iter()
    .filter(|field| field.name() != COVERING_BBOX_COLUMN)
    .map(|field| ident(field.name()))
    .collect();
  if covering {
    expressions.push(ident(COVERING_BBOX_COLUMN));
  }
  debug_assert!(expressions.iter().any(|expression| {
    matches!(expression, Expr::Column(column) if column.name == geometry_column)
  }));
  dataframe.select(expressions).map_err(Into::into)
}

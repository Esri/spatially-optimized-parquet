//! Defines optimized GeoParquet ordering and range-partition planning primitives.

use anyhow::{Result, bail};
use datafusion::logical_expr::expr_fn::ident;
use datafusion::logical_expr::{Expr, SortExpr, when};
use datafusion::prelude::lit;

use crate::analysis::{DisplayJobAnalysis, GeometryFamily};
use crate::output::optimized::clustering::DisplayCode;
use crate::output::optimized::multiscale::{POINT_Z_CODE_COLUMN, TEMP_XZ_CODE_COLUMN};

pub(crate) const POINT_RANGE_COLUMN: &str = "z_order";
pub(crate) const NON_POINT_RANGE_COLUMN: &str = "xz_order";

/// Stores the minimum spatial code and percentile-derived lower range boundaries.
pub(crate) struct RangePartitionBoundaries {
  pub(crate) min_value: DisplayCode,
  pub(crate) boundaries: Vec<DisplayCode>,
}

pub(crate) fn sort_expr(analysis: &DisplayJobAnalysis) -> SortExpr {
  ident(sort_column_name(analysis)).sort(true, false)
}

pub(crate) fn sort_column_name(analysis: &DisplayJobAnalysis) -> &'static str {
  match analysis.geometry_family {
    GeometryFamily::Point => POINT_Z_CODE_COLUMN,
    GeometryFamily::NonPoint => TEMP_XZ_CODE_COLUMN,
  }
}

pub(crate) fn partition_column_name(analysis: &DisplayJobAnalysis) -> &'static str {
  match analysis.geometry_family {
    GeometryFamily::Point => POINT_RANGE_COLUMN,
    GeometryFamily::NonPoint => NON_POINT_RANGE_COLUMN,
  }
}

pub(crate) fn validate_partition_column(
  source_schema: &arrow_schema::Schema,
  partition_column: Option<&str>,
) -> Result<()> {
  if let Some(partition_column) = partition_column
    && source_schema.field_with_name(partition_column).is_ok()
  {
    bail!("output partition column '{partition_column}' conflicts with an existing input column");
  }
  Ok(())
}

pub(crate) fn build_range_partition_expr(
  sort_column: &str,
  min_value: DisplayCode,
  boundaries: &[DisplayCode],
) -> Result<Expr> {
  let mut lower_bounds = Vec::with_capacity(boundaries.len() + 1);
  lower_bounds.push(min_value);
  lower_bounds.extend(boundaries.iter().copied());
  let mut range_expr = lit(*lower_bounds.last().unwrap_or(&min_value));
  for (index, boundary) in boundaries.iter().enumerate().rev() {
    range_expr = when(
      ident(sort_column).lt(lit(*boundary)),
      lit(lower_bounds[index]),
    )
    .otherwise(range_expr)?;
  }
  Ok(range_expr)
}

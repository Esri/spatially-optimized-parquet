//! Defines cluster keys and range partitions for spatially optimized output.

use anyhow::{Result, bail};
use datafusion::logical_expr::expr_fn::ident;
use datafusion::logical_expr::{Expr, SortExpr, when};
use datafusion::prelude::lit;

use crate::optimized::ClusteringFamily;
use crate::optimized::multiscale::{POINT_Z_CODE_COLUMN, TEMP_XZ_CODE_COLUMN};

use super::DisplayCode;

pub(crate) const POINT_RANGE_COLUMN: &str = "z_order";
pub(crate) const NON_POINT_RANGE_COLUMN: &str = "xz_order";

/// Stores the minimum cluster key and percentile-derived lower range boundaries.
pub(crate) struct ClusterRangeBoundaries {
  pub(crate) min_value: DisplayCode,
  pub(crate) boundaries: Vec<DisplayCode>,
}

pub(crate) fn cluster_sort_expr(clustering_family: ClusteringFamily) -> SortExpr {
  ident(cluster_key_column(clustering_family)).sort(true, false)
}

pub(crate) fn cluster_key_column(clustering_family: ClusteringFamily) -> &'static str {
  match clustering_family {
    ClusteringFamily::Point => POINT_Z_CODE_COLUMN,
    ClusteringFamily::NonPoint => TEMP_XZ_CODE_COLUMN,
  }
}

pub(crate) fn cluster_partition_column(clustering_family: ClusteringFamily) -> &'static str {
  match clustering_family {
    ClusteringFamily::Point => POINT_RANGE_COLUMN,
    ClusteringFamily::NonPoint => NON_POINT_RANGE_COLUMN,
  }
}

pub(crate) fn validate_cluster_partition_column(
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

pub(crate) fn build_cluster_range_expr(
  cluster_key_column: &str,
  min_value: DisplayCode,
  boundaries: &[DisplayCode],
) -> Result<Expr> {
  let mut lower_bounds = Vec::with_capacity(boundaries.len() + 1);
  lower_bounds.push(min_value);
  lower_bounds.extend(boundaries.iter().copied());
  let mut range_expr = lit(*lower_bounds.last().unwrap_or(&min_value));
  for (index, boundary) in boundaries.iter().enumerate().rev() {
    range_expr = when(
      ident(cluster_key_column).lt(lit(*boundary)),
      lit(lower_bounds[index]),
    )
    .otherwise(range_expr)?;
  }
  Ok(range_expr)
}

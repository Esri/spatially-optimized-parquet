//! Defines cluster keys and range partitions for spatially optimized output.

use anyhow::{Result, bail};
use datafusion::logical_expr::expr_fn::ident;
use datafusion::logical_expr::{Expr, SortExpr, when};
use datafusion::prelude::lit;

use crate::optimized::ClusteringFamily;
use crate::optimized::multiscale::{POINT_Z_CODE_COLUMN, TEMP_XZ_CODE_COLUMN};

use super::ClusterKey;

const POINT_GEOMETRY_RANGE_COLUMN: &str = "z_order";
const COMPLEX_GEOMETRY_RANGE_COLUMN: &str = "xz_order";

/// Stores the minimum cluster key and percentile-derived lower range boundaries.
pub(crate) struct ClusterRangeBoundaries {
  min_value: ClusterKey,
  boundaries: Vec<ClusterKey>,
}

pub(crate) fn cluster_sort_expr(clustering_family: ClusteringFamily) -> SortExpr {
  ident(cluster_key_column(clustering_family)).sort(true, false)
}

pub(crate) fn cluster_key_column(clustering_family: ClusteringFamily) -> &'static str {
  match clustering_family {
    ClusteringFamily::PointGeometry => POINT_Z_CODE_COLUMN,
    ClusteringFamily::ComplexGeometry => TEMP_XZ_CODE_COLUMN,
  }
}

pub(crate) fn cluster_partition_column(clustering_family: ClusteringFamily) -> &'static str {
  match clustering_family {
    ClusteringFamily::PointGeometry => POINT_GEOMETRY_RANGE_COLUMN,
    ClusteringFamily::ComplexGeometry => COMPLEX_GEOMETRY_RANGE_COLUMN,
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

impl ClusterRangeBoundaries {
  pub(crate) fn new(min_value: u64, boundaries: Vec<u64>) -> Self {
    Self {
      min_value: ClusterKey::new(min_value),
      boundaries: boundaries.into_iter().map(ClusterKey::new).collect(),
    }
  }

  pub(crate) fn partition_expr(&self, cluster_key_column: &str) -> Result<Expr> {
    let mut lower_bounds = Vec::with_capacity(self.boundaries.len() + 1);
    lower_bounds.push(self.min_value);
    lower_bounds.extend(self.boundaries.iter().copied());
    let mut range_expr = lit(
      lower_bounds
        .last()
        .copied()
        .unwrap_or(self.min_value)
        .value(),
    );
    for (index, boundary) in self.boundaries.iter().enumerate().rev() {
      range_expr = when(
        ident(cluster_key_column).lt(lit(boundary.value())),
        lit(lower_bounds[index].value()),
      )
      .otherwise(range_expr)?;
    }
    Ok(range_expr)
  }
}

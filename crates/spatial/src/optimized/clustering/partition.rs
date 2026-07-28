//! Defines cluster keys and range partitions for spatially optimized output.

use datafusion::error::Result as DataFusionResult;
use datafusion::logical_expr::expr_fn::ident;
use datafusion::logical_expr::{Expr, SortExpr, when};
use datafusion::prelude::lit;

use crate::optimized::ClusteringFamily;
use crate::optimized::multiscale::GEOKEY_COLUMN;

use super::ClusterKey;

const POINT_GEOMETRY_RANGE_COLUMN: &str = "z_order";
const COMPLEX_GEOMETRY_RANGE_COLUMN: &str = "xz_order";

/// Defines the minimum cluster key and percentile-derived lower range boundaries.
pub(crate) struct ClusterRangeBoundaries {
  min_value: ClusterKey,
  boundaries: Vec<ClusterKey>,
}

impl ClusteringFamily {
  /// Build the ascending sort expression for this clustering strategy.
  pub(crate) fn sort_expr(self) -> SortExpr {
    ident(self.cluster_key_column()).sort(true, false)
  }

  /// Return the generated cluster-key column for this clustering strategy.
  pub(crate) fn cluster_key_column(self) -> &'static str {
    GEOKEY_COLUMN
  }

  /// Return the generated range-partition column for this clustering strategy.
  pub(crate) fn cluster_partition_column(self) -> &'static str {
    match self {
      Self::PointGeometry => POINT_GEOMETRY_RANGE_COLUMN,
      Self::ComplexGeometry => COMPLEX_GEOMETRY_RANGE_COLUMN,
    }
  }

  /// Reject an input schema that already owns this strategy's partition column.
  pub(crate) fn validate_partition_column(
    self,
    source_schema: &arrow_schema::Schema,
  ) -> Result<(), String> {
    let partition_column = self.cluster_partition_column();
    if source_schema.field_with_name(partition_column).is_ok() {
      return Err(format!(
        "output partition column '{partition_column}' conflicts with an existing input column"
      ));
    }
    Ok(())
  }
}

impl ClusterRangeBoundaries {
  pub(crate) fn new(min_value: u64, boundaries: Vec<u64>) -> Self {
    Self {
      min_value: ClusterKey::new(min_value),
      boundaries: boundaries.into_iter().map(ClusterKey::new).collect(),
    }
  }

  pub(crate) fn partition_expr(&self, cluster_key_column: &str) -> DataFusionResult<Expr> {
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

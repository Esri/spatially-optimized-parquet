//! Defines spatial cluster keys and range partitioning for optimized output.

mod key;
mod partition;
mod xz;
mod z;

use key::ClusterKey;
pub(in crate::optimized) use partition::{
  ClusterRangeBoundaries, cluster_key_column, cluster_partition_column, cluster_sort_expr,
  validate_cluster_partition_column,
};
pub(crate) use xz::bounds_expr;
pub(in crate::optimized) use xz::{DEFAULT_XZ_MAX_LEVEL, non_point_xzcode_from_bounds_expr};
pub(crate) use z::point_expr;
pub(in crate::optimized) use z::{DEFAULT_COORDINATE_PRECISION, point_zcode_from_xy_expr};

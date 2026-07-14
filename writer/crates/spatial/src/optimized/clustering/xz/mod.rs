//! Exposes extent clustering algorithms and typed DataFusion expressions.

mod algorithm;
mod datafusion;

pub(crate) use algorithm::{DEFAULT_XZ_MAX_LEVEL, extent_xz_code};
pub(crate) use datafusion::bounds_expr;
pub(in crate::optimized) use datafusion::non_point_xzcode_from_bounds_expr;

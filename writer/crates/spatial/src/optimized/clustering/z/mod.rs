//! Exposes point clustering algorithms and typed DataFusion expressions.

mod algorithm;
mod datafusion;

pub(in crate::optimized) use algorithm::DEFAULT_COORDINATE_PRECISION;
pub(crate) use datafusion::point_expr;
pub(in crate::optimized) use datafusion::point_zcode_from_xy_expr;

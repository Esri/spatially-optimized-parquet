//! Exposes optimized reprojection planning and typed DataFusion expressions.

mod datafusion;
mod plan;

pub(crate) use datafusion::{
  reproject_geometry_expr, transformed_bounds_expr, transformed_point_coords_expr,
};
pub use plan::{PreparedTransform, ReprojectionPlan, TransformSpec};

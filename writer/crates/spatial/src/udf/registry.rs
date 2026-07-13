use datafusion::execution::context::SessionContext;

use super::clustering::{
  bounds_xmax_udf, bounds_xmin_udf, bounds_ymax_udf, bounds_ymin_udf, non_point_xzcode_udf,
  point_x_udf, point_y_udf, point_zcode_udf,
};

/// Register reusable stateless geometry UDFs in a DataFusion session.
///
/// Parameterized reprojection and multiscale UDFs are embedded directly in expressions.
pub fn register_display_udfs(ctx: &SessionContext) {
  for udf in [
    point_zcode_udf(),
    point_x_udf(),
    point_y_udf(),
    non_point_xzcode_udf(),
    bounds_xmin_udf(),
    bounds_ymin_udf(),
    bounds_xmax_udf(),
    bounds_ymax_udf(),
  ] {
    ctx.register_udf(udf);
  }
}

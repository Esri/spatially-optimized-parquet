//! Names generated and intermediate columns used by optimized multiscale output.

/// Names the generated point Z-order code column.
pub(crate) const POINT_Z_CODE_COLUMN: &str = "zCode";
/// Names the generated point x-coordinate column.
pub(crate) const POINT_X_COLUMN: &str = "x";
/// Names the generated point y-coordinate column.
pub(crate) const POINT_Y_COLUMN: &str = "y";
/// Names the generated non-point display struct column.
pub(crate) const DISPLAY_COLUMN: &str = "geodisplay";
/// Names the XZ-order field within the display struct.
pub(crate) const XZ_CODE_COLUMN: &str = "xzCode";
/// Names the bounds field within the display struct.
pub(crate) const BOUNDS_COLUMN: &str = "bounds";
/// Names the optional GeoParquet 1.1 covering bbox column.
pub(crate) const COVERING_BBOX_COLUMN: &str = "bbox";
/// Names the temporary transformed point-coordinate struct.
pub(crate) const TEMP_POINT_COORDS_COLUMN: &str = "__display_point_coords";
/// Names the temporary transformed bounds struct.
pub(crate) const TEMP_BOUNDS_COLUMN: &str = "__display_bounds";
/// Names the temporary target-CRS WKB column.
pub(crate) const TEMP_REPROJECTED_GEOMETRY_COLUMN: &str = "__display_reprojected_geometry";
/// Names the temporary XZ-order scalar column.
pub(crate) const TEMP_XZ_CODE_COLUMN: &str = "__display_xzcode";
/// Names the temporary minimum-x scalar column.
pub(crate) const TEMP_XMIN_COLUMN: &str = "__display_xmin";
/// Names the temporary minimum-y scalar column.
pub(crate) const TEMP_YMIN_COLUMN: &str = "__display_ymin";
/// Names the temporary maximum-x scalar column.
pub(crate) const TEMP_XMAX_COLUMN: &str = "__display_xmax";
/// Names the temporary maximum-y scalar column.
pub(crate) const TEMP_YMAX_COLUMN: &str = "__display_ymax";

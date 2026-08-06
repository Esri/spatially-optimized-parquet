//! Names generated and intermediate columns used by optimized multiscale output.

/// Defines the persisted spatial ordering key column.
pub(crate) const GEOKEY_COLUMN: &str = "geokey";
/// Defines the generated point x-coordinate column.
pub(crate) const POINT_X_COLUMN: &str = "x";
/// Defines the generated point y-coordinate column.
pub(crate) const POINT_Y_COLUMN: &str = "y";
/// Defines the generated point z-coordinate column.
pub(crate) const POINT_Z_COLUMN: &str = "z";
/// Defines the generated point m-coordinate column.
pub(crate) const POINT_M_COLUMN: &str = "m";
/// Defines the persisted point coordinate struct column.
pub(crate) const SOP_GEOMETRY_COLUMN: &str = "sop_geometry";
/// Defines the persisted multiscale geometry struct column.
pub(crate) const GEOLOD_COLUMN: &str = "geolod";
/// Defines the legacy generated geodisplay struct column.
pub(crate) const GEODISPLAY_COLUMN: &str = "geodisplay";

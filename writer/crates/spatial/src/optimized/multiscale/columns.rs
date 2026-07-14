//! Names generated and intermediate columns used by optimized multiscale output.

use anyhow::{Result, bail};
use arrow_schema::Schema;

/// Names the generated point Z-order code column.
pub(in crate::optimized) const POINT_Z_CODE_COLUMN: &str = "zCode";
/// Names the generated point x-coordinate column.
pub(in crate::optimized) const POINT_X_COLUMN: &str = "x";
/// Names the generated point y-coordinate column.
pub(in crate::optimized) const POINT_Y_COLUMN: &str = "y";
/// Names the generated non-point geodisplay struct column.
pub(in crate::optimized) const GEODISPLAY_COLUMN: &str = "geodisplay";
/// Names the XZ-order field within the geodisplay struct.
pub(in crate::optimized) const XZ_CODE_COLUMN: &str = "xzCode";
/// Names the bounds field within the geodisplay struct.
pub(in crate::optimized) const BOUNDS_COLUMN: &str = "bounds";
/// Names the optional GeoParquet 1.1 covering bbox column.
pub(crate) const COVERING_BBOX_COLUMN: &str = "bbox";
/// Names the temporary transformed point-coordinate struct.
pub(crate) const TEMP_POINT_COORDS_COLUMN: &str = "__clustering_point_coords";
/// Names the temporary transformed bounds struct.
pub(crate) const TEMP_BOUNDS_COLUMN: &str = "__clustering_bounds";
/// Names the temporary target-CRS WKB column.
pub(crate) const TEMP_REPROJECTED_GEOMETRY_COLUMN: &str = "__reprojected_geometry";
/// Names the temporary XZ-order scalar column.
pub(in crate::optimized) const TEMP_XZ_CODE_COLUMN: &str = "__clustering_xzcode";
/// Names the temporary minimum-x scalar column.
pub(crate) const TEMP_XMIN_COLUMN: &str = "__clustering_xmin";
/// Names the temporary minimum-y scalar column.
pub(crate) const TEMP_YMIN_COLUMN: &str = "__clustering_ymin";
/// Names the temporary maximum-x scalar column.
pub(crate) const TEMP_XMAX_COLUMN: &str = "__clustering_xmax";
/// Names the temporary maximum-y scalar column.
pub(crate) const TEMP_YMAX_COLUMN: &str = "__clustering_ymax";

const INTERNAL_PROJECTION_COLUMNS: [&str; 8] = [
  TEMP_POINT_COORDS_COLUMN,
  TEMP_BOUNDS_COLUMN,
  TEMP_REPROJECTED_GEOMETRY_COLUMN,
  TEMP_XZ_CODE_COLUMN,
  TEMP_XMIN_COLUMN,
  TEMP_YMIN_COLUMN,
  TEMP_XMAX_COLUMN,
  TEMP_YMAX_COLUMN,
];

/// Reject source columns reserved for internal projection state.
pub(crate) fn validate_internal_projection_columns(schema: &Schema) -> Result<()> {
  if let Some(column) = INTERNAL_PROJECTION_COLUMNS
    .iter()
    .find(|column| schema.field_with_name(column).is_ok())
  {
    bail!("input column '{column}' conflicts with an internal projection column");
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use arrow_schema::{DataType, Field, Schema};

  use super::{TEMP_BOUNDS_COLUMN, validate_internal_projection_columns};

  #[test]
  fn rejects_internal_projection_column_conflicts() {
    let schema = Schema::new(vec![Field::new(TEMP_BOUNDS_COLUMN, DataType::Binary, true)]);

    let error = validate_internal_projection_columns(&schema).unwrap_err();

    assert!(error.to_string().contains(TEMP_BOUNDS_COLUMN));
  }
}

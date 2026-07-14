//! Names generated and intermediate columns used by optimized multiscale output.

use anyhow::{Result, bail};
use arrow_schema::Schema;

/// Names the generated point Z-order code column.
pub(crate) const POINT_Z_CODE_COLUMN: &str = "zCode";
/// Names the generated point x-coordinate column.
pub(crate) const POINT_X_COLUMN: &str = "x";
/// Names the generated point y-coordinate column.
pub(crate) const POINT_Y_COLUMN: &str = "y";
/// Names the generated non-point geodisplay struct column.
pub(crate) const GEODISPLAY_COLUMN: &str = "geodisplay";
/// Names the XZ-order field within the geodisplay struct.
pub(crate) const XZ_CODE_COLUMN: &str = "xzCode";
/// Names the optional GeoParquet 1.1 covering bbox column.
pub(crate) const COVERING_BBOX_COLUMN: &str = "bbox";
/// Names the temporary XZ-order scalar column.
pub(crate) const TEMP_XZ_CODE_COLUMN: &str = "__clustering_xzcode";

const INTERNAL_PROJECTION_COLUMNS: [&str; 1] = [TEMP_XZ_CODE_COLUMN];

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

  use super::{TEMP_XZ_CODE_COLUMN, validate_internal_projection_columns};

  #[test]
  fn rejects_internal_projection_column_conflicts() {
    let schema = Schema::new(vec![Field::new(
      TEMP_XZ_CODE_COLUMN,
      DataType::UInt64,
      false,
    )]);

    let error = validate_internal_projection_columns(&schema).unwrap_err();

    assert!(error.to_string().contains(TEMP_XZ_CODE_COLUMN));
  }
}

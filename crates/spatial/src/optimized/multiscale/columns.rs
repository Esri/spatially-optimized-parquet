//! Names generated and intermediate columns used by optimized multiscale output.

use anyhow::{Result, bail};
use arrow_schema::Schema;

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

const GENERATED_OUTPUT_COLUMNS: [&str; 8] = [
  GEOKEY_COLUMN,
  SOP_GEOMETRY_COLUMN,
  GEOLOD_COLUMN,
  POINT_X_COLUMN,
  POINT_Y_COLUMN,
  POINT_Z_COLUMN,
  POINT_M_COLUMN,
  "__clustering_xzcode",
];

/// Reject source columns reserved for internal projection state.
pub(crate) fn validate_internal_projection_columns(schema: &Schema) -> Result<()> {
  if let Some(column) = GENERATED_OUTPUT_COLUMNS
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

  use super::{GEOKEY_COLUMN, validate_internal_projection_columns};

  #[test]
  fn rejects_internal_projection_column_conflicts() {
    let schema = Schema::new(vec![Field::new(GEOKEY_COLUMN, DataType::UInt64, false)]);

    let error = validate_internal_projection_columns(&schema).unwrap_err();

    assert!(error.to_string().contains(GEOKEY_COLUMN));
  }
}

//! Owns geodisplay columns, level planning, geometry traversal, and multiscale encoding.

mod columns;
mod datafusion;
mod encoding;
mod level_array_builder;
mod levels;
mod traversal;

pub(crate) use columns::{COVERING_BBOX_COLUMN, validate_internal_projection_columns};
pub(crate) use columns::{
  GEODISPLAY_COLUMN, POINT_M_COLUMN, POINT_X_COLUMN, POINT_Y_COLUMN, POINT_Z_CODE_COLUMN,
  POINT_Z_COLUMN, TEMP_XZ_CODE_COLUMN, XZ_CODE_COLUMN,
};
pub(crate) use datafusion::{ComplexGeometryGeodisplayUdf, PointGeometryGeodisplayUdf};
pub use encoding::MultiscaleEncoding;
pub(crate) use levels::MultiscaleLevel;
pub(crate) use traversal::{GeometryPartRole, GeometryPartSink};

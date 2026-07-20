//! Owns geodisplay columns, level planning, geometry traversal, and multiscale encoding.

mod columns;
mod datafusion;
mod encoding;
mod level_array_builder;
mod levels;
mod traversal;

pub(crate) use columns::validate_internal_projection_columns;
pub(crate) use columns::{
  GEOKEY_COLUMN, GEOLOD_COLUMN, POINT_M_COLUMN, POINT_X_COLUMN, POINT_Y_COLUMN, POINT_Z_COLUMN,
  SOP_GEOMETRY_COLUMN,
};
pub(crate) use datafusion::{GeolodUdf, SopGeometryUdf};
pub use encoding::MultiscaleEncoding;
pub(crate) use levels::MultiscaleLevel;
pub(crate) use traversal::{GeometryPartRole, GeometryPartSink};

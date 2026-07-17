//! Owns geodisplay columns, level planning, geometry traversal, and multiscale encoding.

mod array_builder;
mod columns;
mod datafusion;
mod levels;
mod native;
mod payload;
mod quantize;
mod traversal;
mod wire;

pub(crate) use columns::{COVERING_BBOX_COLUMN, validate_internal_projection_columns};
pub(crate) use columns::{
  GEODISPLAY_COLUMN, POINT_M_COLUMN, POINT_X_COLUMN, POINT_Y_COLUMN, POINT_Z_CODE_COLUMN,
  POINT_Z_COLUMN, TEMP_XZ_CODE_COLUMN, XZ_CODE_COLUMN,
};
pub(in crate::optimized) use datafusion::{non_point_geodisplay_expr, point_geodisplay_expr};
pub(in crate::optimized) use levels::{GeometryEncoding, create_geometry_encodings};
pub(crate) use native::native_coordinate_column_paths;
pub(crate) use native::native_geometry_data_type;
use payload::flat_geometry_payload_from_wkb;
pub(crate) use traversal::{
  GeometryPartRole, GeometryPartSink, geometry_extent_from_wkb, visit_wkb_geometry,
  visit_wkb_geometry_for_display,
};
pub(crate) use wire::decode_pbf_geometry;

#[cfg(test)]
use payload::geometry_payload_from_geometry;

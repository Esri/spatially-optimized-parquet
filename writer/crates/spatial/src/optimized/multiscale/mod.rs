//! Owns geodisplay columns, level planning, geometry traversal, and PBF encoding.

mod columns;
mod datafusion;
mod levels;
mod payload;
mod quantize;
mod traversal;
mod wire;

pub(in crate::optimized) use columns::{
  BOUNDS_COLUMN, GEODISPLAY_COLUMN, POINT_X_COLUMN, POINT_Y_COLUMN, POINT_Z_CODE_COLUMN,
  TEMP_XZ_CODE_COLUMN, XZ_CODE_COLUMN,
};
pub(crate) use columns::{
  COVERING_BBOX_COLUMN, TEMP_BOUNDS_COLUMN, TEMP_POINT_COORDS_COLUMN,
  TEMP_REPROJECTED_GEOMETRY_COLUMN, TEMP_XMAX_COLUMN, TEMP_XMIN_COLUMN, TEMP_YMAX_COLUMN,
  TEMP_YMIN_COLUMN, validate_internal_projection_columns,
};
pub(in crate::optimized) use datafusion::non_point_geodisplay_expr;
pub(in crate::optimized) use levels::{GeometryEncoding, create_geometry_encodings};
use payload::flat_geometry_payload_from_wkb;
pub(crate) use traversal::geometry_extent_from_wkb;
pub(in crate::optimized) use traversal::point_xy_from_wkb;
use wire::{GeometryEncodeScratch, encode_flat_geometry_with_scratch};

#[cfg(test)]
use payload::geometry_payload_from_geometry;

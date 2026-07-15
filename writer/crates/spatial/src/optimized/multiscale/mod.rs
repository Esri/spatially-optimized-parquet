//! Owns geodisplay columns, level planning, geometry traversal, and PBF encoding.

mod columns;
mod datafusion;
mod levels;
mod payload;
mod quantize;
mod traversal;
mod wire;

pub(crate) use columns::{COVERING_BBOX_COLUMN, validate_internal_projection_columns};
pub(crate) use columns::{
  GEODISPLAY_COLUMN, POINT_X_COLUMN, POINT_Y_COLUMN, POINT_Z_CODE_COLUMN, TEMP_XZ_CODE_COLUMN,
  XZ_CODE_COLUMN,
};
pub(in crate::optimized) use datafusion::{non_point_geodisplay_expr, point_geodisplay_expr};
pub(in crate::optimized) use levels::{GeometryEncoding, create_geometry_encodings};
use payload::flat_geometry_payload_from_wkb;
pub(crate) use traversal::{
  GeometryPartRole, GeometryPartSink, geometry_extent_from_wkb, point_xy_from_wkb,
  visit_wkb_geometry,
};
pub(crate) use wire::decode_pbf_geometry;
use wire::{GeometryEncodeScratch, encode_flat_geometry_with_scratch};

#[cfg(test)]
use payload::geometry_payload_from_geometry;

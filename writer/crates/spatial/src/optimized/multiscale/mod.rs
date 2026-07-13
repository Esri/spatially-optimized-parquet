//! Owns geodisplay columns, level planning, geometry traversal, and PBF encoding.

mod columns;
mod datafusion;
mod levels;
mod payload;
mod quantize;
mod traversal;
mod wire;

pub(crate) use columns::*;
pub(crate) use datafusion::non_point_geodisplay_expr;
pub use levels::{
  DEFAULT_MAX_LEVEL, GeometryEncoding, MultiscaleLevel, QuantizationTransform,
  create_geometry_encodings, metadata_levels,
};
pub use payload::{
  FlatGeometryPayload, GeometryPayload, flat_geometry_payload_from_wkb,
  geometry_payload_from_geometry, geometry_payload_from_wkb,
};
pub use traversal::{geometry_extent_from_wkb, point_xy_from_wkb};
pub use wire::{
  GeometryEncodeScratch, encode_flat_geometry_owned_with_scratch,
  encode_flat_geometry_with_scratch, encode_geometry,
};

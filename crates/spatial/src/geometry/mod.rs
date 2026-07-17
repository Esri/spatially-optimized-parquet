//! Defines the format-neutral geometry model and its physical codecs.
//!
//! [`geometry::Geometry`] carries floating-point coordinates and part boundaries. WKB reads and
//! writes that value, [`quantization`] converts it into integer coordinates, and [`native`] plus
//! [`pbf`] encode those integers as nested Arrow values or Esri PBF bytes. This module owns
//! representation mechanics. Multiscale policy and output selection remain outside it.

mod arrow;
mod extent;
mod geometry;
mod native;
mod pbf;
mod quantization;
mod types;
mod wkb;

pub(crate) use arrow::{GeometryArray, geometry_signature, to_datafusion_error};
pub(crate) use extent::Extent2D;
pub(crate) use geometry::{Coord, Geometry, GeometryType};
pub(crate) use native::{
  NativeGeometryArrayBuilder, native_coordinate_column_paths, native_geometry_data_type,
};
pub(crate) use pbf::{
  GeometryEncodeScratch, PbfArrayBuilder, PbfGeometry, decode_pbf_geometry,
  encode_quantized_geometry_with_scratch,
};
pub(crate) use quantization::{
  ComponentValidity, QuantizationOptions, QuantizationTransform, QuantizedGeometry,
  encode_deltas_xy, quantize_geometry_into,
};
pub(crate) use types::{GeometryColumn, GeometryEncoding, GeometryKind};
#[cfg(test)]
pub(crate) use wkb::write_test_geometry;
pub(crate) use wkb::{
  PolygonRingOrder, WkbCoordinate, WkbPartRole, WkbSink, geometry_kind_from_wkb, read_geometry,
  read_wkb_point_coordinate, strip_wkb_dimensions, visit_wkb_geometry, write_geometry,
};

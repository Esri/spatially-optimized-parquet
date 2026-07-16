//! Defines the smallest format-independent vocabulary shared by the spatial pipeline.
//!
//! [`GeometryKind`] normalizes geometry declarations from GDAL, GeoParquet metadata, and WKB.
//! [`GeometryEncoding`] describes the physical representation stored in Arrow, while
//! [`GeometrySpec`] binds that representation to the selected source column. Input providers
//! produce these values and analysis/output code consumes them without depending on the source
//! format.
//!
//! WKB decoding validates headers and traverses coordinates without allocating geometry objects.
//! Extents, reprojection, and optimized encoding remain owned by their dedicated modules.

mod binary_array;
mod extent;
mod types;
mod wkb;

pub(crate) use binary_array::{
  BinaryValueAccess, geometry_signature, map_geometry_to_binary, map_geometry_to_u64,
  to_datafusion_error,
};
pub(crate) use extent::Extent2D;
pub(crate) use types::{
  GeometryCategory, GeometryEncoding, GeometryKind, GeometryShape, GeometrySpec,
};
#[cfg(test)]
pub(crate) use wkb::write_test_geometry;
pub(crate) use wkb::{
  PolygonRingOrder, WkbCoordinate, WkbPartRole, WkbSink, geometry_kind_from_wkb,
  read_wkb_point_coordinate, strip_wkb_dimensions, visit_wkb_geometry,
};

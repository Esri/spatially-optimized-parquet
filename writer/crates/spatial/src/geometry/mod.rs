//! Defines the smallest format-independent vocabulary shared by the spatial pipeline.
//!
//! [`GeometryKind`] normalizes geometry declarations from GDAL, GeoParquet metadata, and WKB.
//! [`GeometryEncoding`] describes the physical representation stored in Arrow, while
//! [`GeometrySpec`] binds that representation to the selected source column. Input providers
//! produce these values and analysis/output code consumes them without depending on the source
//! format.
//!
//! WKB decoding helpers intentionally determine only the top-level geometry kind. Coordinate
//! traversal, extents, reprojection, and optimized encoding belong to dedicated modules.

mod binary_array;
mod extent;
mod types;

pub(crate) use binary_array::{
  BinaryValueAccess, geometry_signature, map_geometry_to_binary, map_geometry_to_u64,
  to_datafusion_error,
};
pub use extent::Extent2D;
pub use types::{
  GeometryCategory, GeometryEncoding, GeometryKind, GeometryShape, GeometrySpec,
  geometry_kind_from_wkb, geometry_kind_from_wkb_type,
};

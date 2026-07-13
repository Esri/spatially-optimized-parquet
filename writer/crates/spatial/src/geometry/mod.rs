//! Defines the smallest format-independent vocabulary shared by the spatial pipeline.
//!
//! [`GeometryKind`] normalizes geometry declarations from GDAL, GeoParquet metadata, and WKB.
//! [`GeometryEncoding`] describes the physical representation stored in Arrow, while
//! [`GeometrySpec`] binds that representation to the selected source column. Input providers
//! produce these values and analysis/output code consumes them without depending on the source
//! format.
//!
//! WKB decoding helpers intentionally determine only the top-level geometry kind. Coordinate
//! traversal, extents, reprojection, and display encoding belong to dedicated modules.

mod extent;
mod types;

pub use extent::Extent2D;
pub use types::{
  GeometryCategory, GeometryEncoding, GeometryKind, GeometryShape, GeometrySpec,
  geometry_kind_from_wkb, geometry_kind_from_wkb_type,
};

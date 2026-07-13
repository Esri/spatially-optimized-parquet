//! Defines the two GeoParquet products emitted by the writer.
//!
//! [`plain`] preserves source coordinates and omits SOP display optimization. [`optimized`]
//! adds reprojection, spatial ordering, and display payloads. Both workflows share geometry,
//! CRS, extent, covering, and GeoParquet metadata rules through [`geoparquet`].

pub(crate) mod geometry;
pub mod geoparquet;
mod mode;
pub mod optimized;
pub mod plain;
pub(crate) mod reprojection;
mod spatial_reference;
pub(crate) mod write;

pub use mode::GeoParquetOutputMode;
pub(crate) use spatial_reference::validate_output_wkid;
pub use spatial_reference::{DEFAULT_OUTPUT_WKID, WEB_MERCATOR_OUTPUT_WKID};

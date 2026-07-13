//! Defines the two GeoParquet products emitted by the writer.
//!
//! [`plain`] preserves source coordinates and omits SOP display optimization. [`optimized`]
//! adds reprojection, spatial ordering, and display payloads. Both workflows share geometry,
//! CRS, extent, covering, and GeoParquet metadata rules through [`geoparquet`].

pub mod geoparquet;
mod mode;
pub mod optimized;
pub mod plain;
pub(crate) mod write;

pub use mode::GeoParquetOutputMode;

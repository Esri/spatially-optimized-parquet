//! Provides mechanics shared by the GeoParquet output products.
//!
//! Product contracts and stages live in [`crate::geoparquet`] and [`crate::optimized`].

mod mode;
pub(crate) mod reprojection;
mod spatial_reference;
pub(crate) mod stage;

pub use mode::GeoParquetOutputMode;
pub(crate) use spatial_reference::validate_output_wkid;
pub use spatial_reference::{DEFAULT_OUTPUT_WKID, SpatialReferenceInfo, WEB_MERCATOR_OUTPUT_WKID};

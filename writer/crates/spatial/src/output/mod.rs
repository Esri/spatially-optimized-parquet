//! Provides mechanics shared by the GeoParquet output products.
//!
//! Product contracts and pipelines live in [`crate::geoparquet`] and [`crate::pipeline`].

mod mode;
mod parquet_metadata;
pub(crate) mod reprojection;
mod spatial_reference;

pub use mode::GeoParquetOutputMode;
pub use parquet_metadata::{ParquetMetadata, ParquetMetadataSet};
pub(crate) use spatial_reference::validate_output_wkid;
pub use spatial_reference::{DEFAULT_OUTPUT_WKID, SpatialReferenceInfo, WEB_MERCATOR_OUTPUT_WKID};

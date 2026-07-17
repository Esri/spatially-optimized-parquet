//! Provides mechanics shared by the GeoParquet output products.
//!
//! Product contracts and pipelines live in [`crate::geoparquet`] and [`crate::pipeline`].

mod mode;
mod parquet;
mod parquet_partition_exec;
mod parquet_sink;
mod parquet_writer;
mod path;
mod spatial_reference;

#[cfg(test)]
pub(crate) use crate::geometry::QuantizationTransform;
pub use mode::OutputMode;
pub(crate) use parquet::ParquetWriterOptions;
pub(crate) use parquet_writer::ParquetOutputWriter;
pub(crate) use path::OutputPath;
pub use spatial_reference::DEFAULT_OUTPUT_WKID;
pub(crate) use spatial_reference::{
  SpatialReferenceInfo, WEB_MERCATOR_OUTPUT_WKID, validate_output_wkid,
};

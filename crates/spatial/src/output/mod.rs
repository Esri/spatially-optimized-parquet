//! Provides mechanics shared by the GeoParquet output products.
//!
//! Product contracts and pipelines live in [`crate::geoparquet`] and [`crate::pipeline`].

mod mode;
mod parquet;
mod parquet_partition_exec;
mod parquet_sink;
mod parquet_writer;
mod path;

#[cfg(test)]
pub(crate) use crate::geometry::QuantizationTransform;
pub use mode::OutputMode;
pub(crate) use parquet::ParquetWriterOptions;
pub(crate) use parquet_writer::ParquetOutputWriter;
pub(crate) use path::OutputPath;

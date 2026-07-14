//! Provides the execution substrate shared by the spatial optimization pipeline.
//!
//! Geospatial policy intentionally remains outside this crate. The `spatial` crate decides
//! which columns to derive, how geometries should be indexed, and which metadata to emit.
//! The root exports only the session, output-layout, and Parquet services required across the
//! crate boundary.

#![warn(missing_docs)]

mod output_layout;
mod parquet_scan;
mod parquet_write;
mod session;

pub use output_layout::OutputLayout;
pub use parquet_scan::scan_parquet;
pub use parquet_write::{ParquetWriterOptions, write_single_file, written_row_count};
pub use session::DataFusionSession;

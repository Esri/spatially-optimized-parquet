//! Provides the execution substrate shared by the spatial optimization pipeline.
//!
//! The crate separates reusable mechanics into focused modules:
//!
//! - [`session`] configures DataFusion memory, partitioning, and disk spilling.
//! - [`parquet_scan`] constructs local Parquet scans through DataFusion.
//! - [`output_layout`] resolves file-versus-directory output layouts before execution.
//! - [`parquet_write`] configures and executes DataFusion Parquet sinks.
//!
//! Geospatial policy intentionally remains outside this crate. The `spatial` crate decides
//! which columns to derive, how geometries should be indexed, and which metadata to emit.
//! This boundary lets execution settings evolve without coupling them to geometry semantics.

#![warn(missing_docs)]

pub mod output_layout;
pub mod parquet_scan;
pub mod parquet_write;
pub mod session;

pub use arrow_schema::SchemaRef;
pub use datafusion::dataframe::DataFrame;
pub use datafusion::execution::context::SessionContext;
pub use datafusion::physical_plan::SendableRecordBatchStream;
pub use parquet::basic::Compression;
pub use parquet::file::metadata::KeyValue;

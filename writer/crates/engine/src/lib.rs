//! Provides the execution substrate shared by the spatial optimization pipeline.
//!
//! The crate separates reusable mechanics into focused modules:
//!
//! - [`session`] configures DataFusion memory, partitioning, and disk spilling.
//! - [`read`] converts lazy DataFrames into ordered or partitioned batch streams.
//! - [`plan`] validates file-versus-directory output layouts before execution.
//! - [`mod@write`] configures DataFusion Parquet sinks.
//!
//! Geospatial policy intentionally remains outside this crate. The `spatial` crate decides
//! which columns to derive, how geometries should be indexed, and which metadata to emit.
//! This boundary lets execution settings evolve without coupling them to geometry semantics.

#![warn(missing_docs)]

pub mod plan;
pub mod read;
pub mod session;
pub mod write;

pub use arrow_schema::SchemaRef;
pub use datafusion::dataframe::DataFrame;
pub use datafusion::execution::context::SessionContext;
pub use datafusion::physical_plan::SendableRecordBatchStream;
pub use parquet::basic::Compression;
pub use parquet::file::metadata::KeyValue;

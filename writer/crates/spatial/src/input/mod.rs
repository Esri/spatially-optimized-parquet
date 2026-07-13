//! Resolves and opens GeoPackage or Parquet through one explicit source boundary.
//!
//! [`mod@format`] owns source identification while [`source`] owns the shared contract, location
//! classification, and format routing. Parquet sources register HTTP object stores so DataFusion
//! owns ranged reads, decoding, and reusable DataFrame caching.

pub mod format;
pub mod gpkg;
pub mod parquet;
pub mod source;

pub use format::{SourceFormat, resolve_source_format};
pub use source::{InputBatchStream, InputOpenOptions, InputSource, RowRange, open_input};

//! Resolves and opens GeoPackage or Parquet through one explicit source boundary.
//!
//! [`mod@format`] owns source identification while [`source`] owns the shared contract, location
//! classification, and format routing. Parquet sources register HTTP object stores so DataFusion
//! owns ranged reads, decoding, and reusable DataFrame caching.

mod format;
mod gpkg;
mod metadata;
pub(crate) mod parquet;
mod source;

pub use format::SourceFormat;
pub(crate) use metadata::{SourceCoveringMetadata, SourceDatasetMetadata, SourceGeometryMetadata};
pub use source::RowRange;
pub(crate) use source::{InputOpenOptions, InputSource, open_input};

//! Integrates local and HTTP Parquet sources, including GeoParquet metadata normalization.
//!
//! [`ParquetInputSource::open`] accepts one local file, a directory of `.parquet` files, or a direct
//! HTTP(S) URL ending in `.parquet`. Discovery reads every local footer or performs HTTP object
//! metadata and footer range requests. GeoParquet `geo` JSON must remain semantically consistent
//! across a local file set. Reserved metadata stays under writer control, while unrelated
//! key-value pairs can pass through to output.
//!
//! Normal scans delegate to `SessionContext::read_parquet`, so DataFusion owns row-group/page
//! planning, decompression, partition scheduling, limits, and Arrow batch production.
//!
//! Footer discovery cost scales with file count, and HTTP execution can issue new range requests
//! after provider discovery. The source stores loaded footer metadata so schema, row count, and
//! spatial metadata queries do not reopen local files.

mod dataset;
mod metadata;
mod open;
mod source;

pub(crate) use dataset::{
  DiscoveryMode, ParquetDataset, ParquetDatasetFile, PartitionDescriptor, PartitionFamily,
};
pub(super) use source::ParquetInputSource;

#[cfg(test)]
mod tests;

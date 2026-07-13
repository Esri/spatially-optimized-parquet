//! Integrates local and HTTP Parquet sources, including GeoParquet metadata normalization.
//!
//! [`open_source`] accepts one local file, a directory of `.parquet` files, or a direct
//! HTTP(S) URL ending in `.parquet`. Discovery reads every local footer or performs HTTP object
//! metadata and footer range requests. GeoParquet `geo` JSON must remain semantically consistent
//! across a local file set. Reserved metadata stays under writer control, while unrelated
//! key-value pairs can pass through to output.
//!
//! Normal scans delegate to `SessionContext::read_parquet`, so DataFusion owns row-group/page
//! planning, decompression, partition scheduling, limits, and Arrow batch production. Direct
//! `read_batches` uses the same DataFusion path locally and a Parquet object reader over HTTP.
//! The job may materialize very small bounded HTTP ranges once, preventing its independent
//! analysis and write plans from repeating remote reads.
//!
//! Footer discovery cost scales with file count, and HTTP execution can issue new range requests
//! after provider discovery. The source stores loaded footer metadata so schema, row count, and
//! spatial metadata queries do not reopen local files.

mod metadata;
mod open;
mod source;

pub use open::open_source;
pub use source::ParquetInputSource;

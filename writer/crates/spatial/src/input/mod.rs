//! Resolves and opens GeoPackage or Parquet through one explicit source boundary.
//!
//! [`mod@format`] owns source identification, [`source`] owns the shared contract,
//! `dispatch` owns format routing, and `location` owns location classification. Concrete
//! modules own storage integration, so corrupt inputs retain format-specific errors and unknown
//! locations never depend on provider order.

mod dispatch;
pub mod format;
pub mod gpkg;
mod location;
pub(crate) mod materialized;
pub mod parquet;
pub mod source;

pub use dispatch::open_input;
pub use format::{SourceFormat, resolve_source_format};
pub use location::is_http_url;
pub(crate) use location::require_local_path;
pub use source::{InputBatchStream, InputOpenOptions, InputSource, RowRange};

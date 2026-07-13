//! Exposes source metadata and GeoParquet metadata writing.

pub mod source;
mod writer;

pub use source::{SourceDatasetMetadata, SourceGeometryMetadata};
pub use writer::{build_geo_key_values, build_geo_metadata};

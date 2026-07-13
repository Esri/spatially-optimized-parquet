//! Exposes source metadata and GeoParquet metadata writing.

pub mod source;
mod writer;

pub use source::{SourceDatasetMetadata, SourceGeometryMetadata};
pub use writer::{GeoMetadata, GeoMetadataInput};

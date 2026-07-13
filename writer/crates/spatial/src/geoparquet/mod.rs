//! Owns the GeoParquet product contract and plain output computations.

mod covering;
mod geometry_scan;
pub mod metadata;
mod output;
mod source;
mod source_crs;

pub(crate) use covering::{feature_bbox_expr, validate_covering_configuration};
pub use metadata::{GeoMetadata, GeoMetadataInput};
pub(crate) use output::{analyze_plain_target_extent, plain_output_dataframe};
pub use source::{ResolvedGeoParquetSource, resolve_source};

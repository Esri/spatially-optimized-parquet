//! Owns the GeoParquet product contract and plain output stage.

mod covering;
mod geometry_scan;
pub mod metadata;
mod output;
mod source;
mod source_crs;

pub(crate) use covering::{feature_bbox_expr, validate_covering_configuration};
pub use metadata::{GeoMetadataInput, build_geo_key_values, build_geo_metadata};
pub(crate) use output::PlainGeoParquet;
pub use source::{ResolvedGeoParquetSource, resolve_source};

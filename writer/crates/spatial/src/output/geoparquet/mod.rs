//! Exposes shared GeoParquet metadata and covering behavior.

mod context;
mod covering;

pub(crate) use context::validate_covering_configuration;
pub use context::{
  SourceGeoParquetContext, build_geo_key_values, build_geo_metadata, resolve_source_context,
};
pub(crate) use covering::feature_bbox_expr;

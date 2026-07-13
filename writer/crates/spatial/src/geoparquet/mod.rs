//! Owns the GeoParquet product contract and plain output workflow.

mod context;
mod covering;
pub mod metadata;
mod workflow;

pub(crate) use context::validate_covering_configuration;
pub use context::{SourceGeoParquetContext, resolve_source_context};
pub(crate) use covering::feature_bbox_expr;
pub use metadata::{build_geo_key_values, build_geo_metadata};
pub(crate) use workflow::{PlainOutputRequest, write};

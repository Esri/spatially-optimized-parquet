//! Owns the GeoParquet product contract and plain output computations.

mod covering;
mod geo_metadata;
mod geometry_scan;
mod normalized_spatial_frame;
mod reprojection;
mod source;
mod spatial_reference;
mod strip;
mod writer;

pub(crate) use covering::{bbox_field_expr, geometry_bbox_expr};
pub(crate) use geo_metadata::{
  GeoMetadata, GeoMetadataInput, geo_metadata_entry, geoparquet_metadata,
};
pub(crate) use normalized_spatial_frame::NormalizedSpatialFrame;
pub(crate) use reprojection::{ResolvedReprojection, reproject_geometry_expr};
pub(crate) use source::{ResolvedGeoParquetSource, resolve_source};
pub use spatial_reference::DEFAULT_OUTPUT_WKID;
pub(crate) use spatial_reference::{
  SpatialReference, WEB_MERCATOR_OUTPUT_WKID, validate_output_wkid,
};
pub(crate) use strip::strip_geometry_dimensions_expr;
pub(crate) use writer::GeoParquetWriter;

#[cfg(test)]
mod tests;

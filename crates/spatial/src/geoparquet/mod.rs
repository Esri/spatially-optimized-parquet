//! Owns the GeoParquet product contract and plain output computations.

mod context;
mod covering;
mod extent_resolve;
mod geo_metadata;
mod geometry_scan;
mod normalized_spatial_frame;
mod reprojection;
mod source;
mod spatial_reference;
mod strip;
mod writer;

pub(crate) use context::GeoParquetWriteContext;
pub(crate) use covering::{COVERING_BBOX_COLUMN, bbox_field_expr, geometry_bbox_expr};
pub(crate) use extent_resolve::ExtentResolver;
pub(crate) use geo_metadata::{GeoMetadata, GeoMetadataInput};
pub(crate) use normalized_spatial_frame::NormalizedSpatialFrame;
pub(crate) use reprojection::ResolvedReprojection;
pub(crate) use source::{ResolvedGeoParquetSource, resolve_source};
pub use spatial_reference::DEFAULT_OUTPUT_WKID;
pub(crate) use spatial_reference::{SpatialReference, WEB_MERCATOR_OUTPUT_WKID};
pub(crate) use strip::StripGeometryDimensionsUdf;
pub(crate) use writer::GeoParquetWriter;

#[cfg(test)]
mod tests;

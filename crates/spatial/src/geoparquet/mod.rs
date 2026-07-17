//! Owns the GeoParquet product contract and plain output computations.

mod covering;
mod geo_metadata;
mod geometry_scan;
mod normalized_spatial_frame;
mod projection;
mod source;
mod source_crs;
mod writer;

pub(crate) use covering::{bbox_field_expr, geometry_bbox_expr};
pub(crate) use geo_metadata::{
  GeoMetadata, GeoMetadataInput, geo_metadata_entry, geoparquet_metadata,
};
pub(crate) use normalized_spatial_frame::NormalizedSpatialFrame;
use projection::plain_output_dataframe;
pub(crate) use source::{ResolvedGeoParquetSource, resolve_source};
pub(crate) use writer::GeoParquetWriter;

#[cfg(test)]
mod tests;

//! Owns the GeoParquet product contract and plain output computations.

mod covering;
mod geometry_scan;
mod normalized_spatial_frame;
mod projection;
mod source;
mod source_crs;
mod writer;

pub(crate) use covering::{bbox_field_expr, geometry_bbox_expr};
pub(crate) use normalized_spatial_frame::NormalizedSpatialFrame;
pub(crate) use writer::GeoParquetWriter;
use projection::plain_output_dataframe;
pub(crate) use source::{ResolvedGeoParquetSource, resolve_source};

#[cfg(test)]
mod tests;

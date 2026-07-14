//! Owns the GeoParquet product contract and plain output computations.

mod covering;
mod geometry_scan;
mod output;
mod prepared_frame;
mod projection;
mod source;
mod source_crs;

pub(crate) use covering::{bbox_field_expr, geometry_bbox_expr, point_bbox_expr};
pub(crate) use output::PlainOutput;
pub(crate) use prepared_frame::PreparedSpatialFrame;
use projection::plain_output_dataframe;
pub(crate) use source::{ResolvedGeoParquetSource, resolve_source};

#[cfg(test)]
mod tests;

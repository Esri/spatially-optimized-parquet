//! Owns the GeoParquet product contract and plain output computations.

mod covering;
mod geometry_scan;
mod output;
mod plain;
mod source;
mod source_crs;

pub(crate) use covering::{feature_bbox_expr, validate_covering_configuration};
use output::{analyze_plain_target_extent, plain_output_dataframe};
pub(crate) use plain::PlainOutput;
pub(crate) use source::{ResolvedGeoParquetSource, resolve_source};

#[cfg(test)]
mod tests;

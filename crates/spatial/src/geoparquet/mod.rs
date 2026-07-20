//! Owns the GeoParquet product contract and plain output computations.

mod covering;
mod extensions;
mod geo_metadata;
mod spatial_reference;

pub(crate) use covering::{COVERING_BBOX_COLUMN, bbox_field_expr, geometry_bbox_expr};
pub(crate) use extensions::{
  LodEncoding, LodLevel, LodMetadata, LodTransform, OrderingMetadata, XzOrderingMetadata,
  ZOrderingMetadata,
};
pub(crate) use geo_metadata::{GeoMetadata, GeoMetadataInput};
pub use spatial_reference::DEFAULT_OUTPUT_WKID;
pub(crate) use spatial_reference::{SpatialReference, WEB_MERCATOR_OUTPUT_WKID};

#[cfg(test)]
mod tests;

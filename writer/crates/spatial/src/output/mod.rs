//! Provides mechanics shared by the GeoParquet output products.
//!
//! Product contracts and pipelines live in [`crate::geoparquet`] and [`crate::pipeline`].

mod layout;
mod metadata;
mod mode;
mod parquet;
mod parquet_sink;
mod reprojection;
mod spatial_reference;

pub(crate) use layout::OutputLayout;
pub(crate) use metadata::{
  ESRI_PBF_ENCODING, GEODISPLAY_VERSION, GeoMetadata, GeoMetadataInput, GeodisplayMetadata,
  MultiscaleLevelInput, XzClusteringIndex, XzClusteringIndexInput, ZClusteringIndex,
  ZClusteringIndexInput, geoparquet_metadata, optimized_point_metadata, optimized_xz_metadata,
};
pub use mode::OutputMode;
pub(crate) use parquet::ParquetWriterOptions;
pub(crate) use parquet_sink::TrackingParquetWriter;
pub(crate) use reprojection::{
  CoordinateTransformSpec, ReprojectionSpec, reproject_geometry_expr, transformed_bounds_expr,
  transformed_point_coords_expr,
};
pub use spatial_reference::DEFAULT_OUTPUT_WKID;
pub(crate) use spatial_reference::{
  SpatialReferenceInfo, WEB_MERCATOR_OUTPUT_WKID, validate_output_wkid,
};

//! Provides mechanics shared by the GeoParquet output products.
//!
//! Product contracts and pipelines live in [`crate::geoparquet`] and [`crate::pipeline`].

mod dimensions;
mod layout;
mod metadata;
mod mode;
mod multiscale;
mod parquet;
mod parquet_sink;
mod reprojection;
mod spatial_reference;

pub(crate) use dimensions::strip_geometry_dimensions_expr;
pub(crate) use layout::OutputLayout;
#[cfg(test)]
pub(crate) use metadata::QuantizationTransform;
pub(crate) use metadata::{
  ESRI_PBF_ENCODING, GEODISPLAY_VERSION, GeoMetadata, GeoMetadataInput, GeodisplayIndex,
  GeodisplayMetadata, MultiscaleLevel, MultiscaleLevelInput, QUANTIZED_NATIVE_ENCODING,
  XzClusteringIndex, XzClusteringIndexInput, ZClusteringIndex, ZClusteringIndexInput,
  geoparquet_metadata, optimized_point_metadata, optimized_xz_metadata,
};
pub use mode::OutputMode;
pub use multiscale::MultiscaleEncoding;
pub(crate) use parquet::ParquetWriterOptions;
pub(crate) use parquet_sink::TrackingParquetWriter;
pub(crate) use reprojection::{ReprojectionSpec, reproject_geometry_expr};
pub use spatial_reference::DEFAULT_OUTPUT_WKID;
pub(crate) use spatial_reference::{
  SpatialReferenceInfo, WEB_MERCATOR_OUTPUT_WKID, validate_output_wkid,
};

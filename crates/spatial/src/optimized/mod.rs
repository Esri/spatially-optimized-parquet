//! Implements optimized spatial analysis, projection, and physical output mechanics.

mod clustering;
mod extent_resolve;
pub(crate) mod geodisplay_metadata;
mod geometry_info;
mod metadata;
mod multiscale;
pub(crate) mod partitioned;
mod resolve;
mod select;
pub(crate) mod single;

pub(crate) use clustering::{BoundsUdf, ClusterKey, DEFAULT_XZ_MAX_LEVEL, PointGeometryUdf};
pub(crate) use extent_resolve::ExtentResolver;
pub(crate) use geodisplay_metadata::{
  ClusteringIndexXZ, ClusteringIndexZ, ESRI_PBF_ENCODING, GEODISPLAY_VERSION, GeodisplayIndex,
  GeodisplayMetadata, MultiscaleLevel, QUANTIZED_NATIVE_ENCODING,
};
pub use multiscale::MultiscaleEncoding;
pub(crate) use multiscale::{
  COVERING_BBOX_COLUMN, GeometryPartRole, GeometryPartSink, validate_internal_projection_columns,
};
pub(crate) use resolve::{ResolvedOptimization, resolve_optimized_geoparquet};

pub(crate) use geometry_info::{ClusteringFamily, GeometryInfo};

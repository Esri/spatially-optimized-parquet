//! Implements optimized spatial analysis, projection, and physical output mechanics.

mod clustering;
pub(crate) mod geodisplay_metadata;
mod geometry_info;
mod layout;
mod metadata;
mod multiscale;
pub(crate) mod partitioned;
mod select;
pub(crate) mod single;

pub(crate) use clustering::{BoundsUdf, ClusterKey, DEFAULT_XZ_MAX_LEVEL, PointGeometryUdf};
pub(crate) use geodisplay_metadata::{
  ClusteringIndexXZ, ClusteringIndexZ, GEODISPLAY_VERSION, GeodisplayEncoding, GeodisplayIndex,
  GeodisplayMetadata, MultiscaleLevel,
};
pub(crate) use layout::OptimizedLayout;
pub use multiscale::MultiscaleEncoding;
pub(crate) use multiscale::{
  GeometryPartRole, GeometryPartSink, validate_internal_projection_columns,
};

pub(crate) use geometry_info::{ClusteringFamily, GeometryInfo};

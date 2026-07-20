//! Implements optimized spatial analysis, projection, and physical output mechanics.

mod clustering;
pub(crate) mod geodisplay_metadata;
mod geometry_info;
mod layout;
mod metadata;
mod multiscale;
mod select;

pub(crate) use clustering::{
  BoundsUdf, ClusterKey, ClusterRangeBoundaries, DEFAULT_XZ_MAX_LEVEL, PointGeometryUdf,
};
pub(crate) use geodisplay_metadata::{
  ClusteringIndexXZ, ClusteringIndexZ, ColumnPath, GEODISPLAY_VERSION, GeodisplayEncoding,
  GeodisplayMetadata, MultiscaleLevel,
};
pub(crate) use layout::OptimizedLayout;
#[cfg(test)]
pub(crate) use multiscale::GEOKEY_COLUMN;
pub use multiscale::MultiscaleEncoding;
pub(crate) use multiscale::{
  GeometryPartRole, GeometryPartSink, validate_internal_projection_columns,
};

pub(crate) use geometry_info::{ClusteringFamily, GeometryInfo};

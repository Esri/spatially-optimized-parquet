//! Implements optimized spatial analysis, projection, and physical output mechanics.

mod clustering;
mod extent;
mod geometry;
mod metadata;
mod multiscale;
mod output;
mod partitioned_sort;
mod projection;
mod range_boundaries;
mod state;
mod write;

pub(crate) use clustering::{
  DEFAULT_XZ_MAX_LEVEL, bounds_expr, extent_xz_code, point_expr, point_z_code,
};
pub(crate) use extent::TargetExtentResolver;
pub(crate) use multiscale::{
  COVERING_BBOX_COLUMN, GeometryPartRole, GeometryPartSink, decode_pbf_geometry,
  geometry_extent_from_wkb, point_xy_from_wkb, validate_internal_projection_columns,
  visit_wkb_geometry,
};
pub(crate) use output::OptimizedOutput;

use geometry::{ClusteringFamily, OptimizedGeometry, OptimizedGeometryType};
use state::ResolvedOptimization;

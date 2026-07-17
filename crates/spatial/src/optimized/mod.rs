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
pub(crate) use geometry::OptimizedGeometryType;
pub(crate) use multiscale::{
  COVERING_BBOX_COLUMN, GeometryPartRole, GeometryPartSink, decode_pbf_geometry,
  geometry_extent_from_wkb, native_geometry_data_type, validate_internal_projection_columns,
  visit_wkb_geometry, visit_wkb_geometry_for_display,
};
pub(crate) use output::OptimizedOutput;

use geometry::{ClusteringFamily, OptimizedGeometry};
use state::ResolvedOptimization;

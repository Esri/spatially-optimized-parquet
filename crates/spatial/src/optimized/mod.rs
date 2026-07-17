//! Implements optimized spatial analysis, projection, and physical output mechanics.

mod clustering;
mod extent_resolve;
mod geometry_info;
mod metadata;
mod multiscale;
pub(crate) mod partitioned;
mod resolve;
mod select;
pub(crate) mod single;

pub(crate) use clustering::{
  DEFAULT_XZ_MAX_LEVEL, bounds_expr, extent_xz_code, point_expr, point_z_code,
};
pub(crate) use extent_resolve::ExtentResolver;
pub(crate) use multiscale::{
  COVERING_BBOX_COLUMN, GeometryPartRole, GeometryPartSink, geometry_extent_from_wkb,
  validate_internal_projection_columns, visit_wkb_geometry, visit_wkb_geometry_for_display,
};
pub(crate) use resolve::{ResolvedOptimization, resolve_optimized_geoparquet};

pub(crate) use geometry_info::{ClusteringFamily, GeometryInfo};

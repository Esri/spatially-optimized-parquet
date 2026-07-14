//! Implements optimized spatial analysis, projection, and physical output mechanics.

mod aggregate;
mod clustering;
mod extent;
mod geometry;
mod metadata;
mod multiscale;
mod output;
mod partitioned_sink;
mod partitioned_sort;
mod projection;
mod range_boundaries;
mod state;
mod write;

pub(crate) use clustering::{
  DEFAULT_XZ_MAX_LEVEL, bounds_expr, extent_xz_code, point_expr, point_z_code,
};
pub(crate) use multiscale::{
  COVERING_BBOX_COLUMN, GeometryPartRole, GeometryPartSink, TEMP_BOUNDS_COLUMN,
  TEMP_POINT_COORDS_COLUMN, TEMP_REPROJECTED_GEOMETRY_COLUMN, TEMP_XMAX_COLUMN, TEMP_XMIN_COLUMN,
  TEMP_YMAX_COLUMN, TEMP_YMIN_COLUMN, decode_pbf_geometry, geometry_extent_from_wkb,
  point_xy_from_wkb, validate_internal_projection_columns, visit_wkb_geometry,
};
pub(crate) use output::OptimizedOutput;

use geometry::{ClusteringFamily, OptimizedGeometry, OptimizedGeometryType};
use state::ResolvedOptimization;

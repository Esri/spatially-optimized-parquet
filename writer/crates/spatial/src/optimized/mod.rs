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

pub(crate) use clustering::{bounds_expr, point_expr};
pub(crate) use multiscale::{
  COVERING_BBOX_COLUMN, TEMP_BOUNDS_COLUMN, TEMP_POINT_COORDS_COLUMN,
  TEMP_REPROJECTED_GEOMETRY_COLUMN, TEMP_XMAX_COLUMN, TEMP_XMIN_COLUMN, TEMP_YMAX_COLUMN,
  TEMP_YMIN_COLUMN, geometry_extent_from_wkb, validate_internal_projection_columns,
};
pub(crate) use output::OptimizedOutput;

use geometry::{ClusteringFamily, OptimizedGeometry, OptimizedGeometryType};
use state::ResolvedOptimization;

//! Implements optimized spatial analysis, projection, and physical output mechanics.

pub(crate) mod aggregate;
pub(crate) mod clustering;
pub(crate) mod extent;
pub mod geometry;
pub mod metadata;
pub mod multiscale;
pub(crate) mod partitioned_sink;
pub(crate) mod partitioned_sort;
pub(crate) mod projection;
pub(crate) mod range_boundaries;
mod state;
pub(crate) mod write;

pub use geometry::{ClusteringFamily, OptimizedGeometry, OptimizedGeometryType};
pub use state::ResolvedOptimization;

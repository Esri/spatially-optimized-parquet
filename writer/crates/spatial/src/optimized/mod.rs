//! Executes the Spatially Optimized GeoParquet stage behind one output boundary.

pub(crate) mod clustering;
mod context;
pub(crate) mod execution;
pub(crate) mod extent;
pub mod geometry;
pub mod metadata;
pub mod multi_file;
pub mod multiscale;
mod output;
pub(crate) mod partitioning;
pub(crate) mod projection;
pub(crate) mod write;

pub use context::OptimizedContext;
pub use geometry::{ClusteringFamily, OptimizedGeometry, OptimizedGeometryType};
pub(crate) use output::OptimizedGeoParquet;

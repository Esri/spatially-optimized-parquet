//! Executes the Spatially Optimized GeoParquet workflow behind one output boundary.

pub(crate) mod analysis;
pub(crate) mod clustering;
pub(crate) mod dataframe;
pub(crate) mod execution;
pub mod metadata;
pub mod multi_file;
pub mod multiscale;
pub(crate) mod partitioning;
pub(crate) mod prepare;
mod run;
mod workflow;
pub(crate) mod write;

pub(crate) use run::run;
pub(crate) use workflow::OptimizeOutputRequest;

//! Builds and writes range-partitioned optimized GeoParquet.

mod dataframe;
mod range;
mod sort;
mod write;

pub(crate) use write::write;

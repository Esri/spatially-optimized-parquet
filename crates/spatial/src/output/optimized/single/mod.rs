//! Builds and writes globally ordered single-file optimized GeoParquet.

mod dataframe;
mod write;

pub(crate) use write::write;

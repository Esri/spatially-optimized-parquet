//! Provides mechanics shared by the GeoParquet output products.
//!
//! Product contracts and pipelines live in [`crate::geoparquet`] and [`crate::pipeline`].

mod mode;
mod partition_plan;
mod path;
mod reporter;
mod tracking_sink;
mod writer;

#[cfg(test)]
pub(crate) use crate::geometry::QuantizationTransform;
pub use mode::OutputMode;
pub(crate) use path::OutputPath;
pub(crate) use writer::{Writer, WriterOptions};

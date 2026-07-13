//! Exposes geometry computation as typed DataFusion scalar expressions.
//!
//! Expression builders form the supported surface. Internal modules own DataFusion UDF
//! implementations, cached signatures, and shared Arrow/WKB adapters.

mod builders;
mod clustering;
mod multiscale;
mod registry;
mod reprojection;
mod signatures;
mod support;

pub use builders::*;
pub use registry::register_display_udfs;

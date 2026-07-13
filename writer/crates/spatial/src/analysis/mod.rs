//! Determines the geometry facts required before an optimization plan can be built.
//!
//! Analysis resolves the selected geometry into a supported display category, point/non-point
//! strategy, full extent, coordinate reference system, and Z/M dimensionality. Complete source
//! metadata provides a constant-time fast path. Row limits, missing extents, ambiguous geometry
//! declarations, or reprojection invalidate that path and force a WKB scan.
//!
//! The scan accepts all Arrow binary representations used by providers, reports progress in
//! bounded chunks, rejects mixed display categories, and merges per-feature bounds. Transform
//! specifications can calculate extents in the target CRS during analysis, ensuring later
//! spatial codes use the same coordinate space as output geometry.

mod api;
mod derive;
mod types;

pub use api::{
  analyze_display_job, analyze_display_job_with_progress,
  analyze_display_job_with_progress_and_transform,
};
pub use types::{
  DisplayGeometryType, DisplayJobAnalysis, Extent2D, GeometryFamily, SpatialReferenceInfo,
};

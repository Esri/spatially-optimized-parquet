//! Stores the complete semantic context for optimized output.

use crate::geometry::Extent2D;
use crate::geoparquet::metadata::source::SourceDatasetMetadata;
use crate::optimized::multiscale::GeometryEncoding;
use crate::output::reprojection::ReprojectionContext;

use super::geometry::OptimizedGeometry;

#[derive(Debug, Clone)]
/// Stores resolved source, geometry, projection, extent, and encoding state for optimized output.
pub struct OptimizedContext {
  pub(crate) source_metadata: SourceDatasetMetadata,
  pub(crate) geometry: OptimizedGeometry,
  pub(crate) reprojection: ReprojectionContext,
  pub(crate) target_extent: Extent2D,
  pub(crate) encodings: Vec<GeometryEncoding>,
}

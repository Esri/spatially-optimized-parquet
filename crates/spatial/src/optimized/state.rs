//! Stores the complete resolved state for optimized output.

use crate::geometry::Extent2D;
use crate::input::SourceDatasetMetadata;
use crate::optimized::multiscale::GeometryEncoding;
use crate::output::ReprojectionSpec;

use super::geometry::OptimizedGeometry;

#[derive(Debug, Clone)]
/// Stores resolved source, geometry, projection, extent, and encoding state for optimized output.
pub(super) struct ResolvedOptimization {
  source_metadata: SourceDatasetMetadata,
  geometry: OptimizedGeometry,
  reprojection: ReprojectionSpec,
  target_extent: Extent2D,
  encodings: Vec<GeometryEncoding>,
}

impl ResolvedOptimization {
  pub(super) fn new(
    source_metadata: SourceDatasetMetadata,
    geometry: OptimizedGeometry,
    reprojection: ReprojectionSpec,
    target_extent: Extent2D,
    encodings: Vec<GeometryEncoding>,
  ) -> Self {
    Self {
      source_metadata,
      geometry,
      reprojection,
      target_extent,
      encodings,
    }
  }

  pub(super) fn source_metadata(&self) -> &SourceDatasetMetadata {
    &self.source_metadata
  }

  pub(super) fn geometry(&self) -> &OptimizedGeometry {
    &self.geometry
  }

  pub(super) fn reprojection(&self) -> &ReprojectionSpec {
    &self.reprojection
  }

  pub(super) fn target_extent(&self) -> Extent2D {
    self.target_extent
  }

  pub(super) fn encodings(&self) -> &[GeometryEncoding] {
    &self.encodings
  }
}

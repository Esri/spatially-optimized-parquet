//! Stores the complete resolved state for optimized output.

use crate::geometry::Extent2D;
use crate::input::SourceDatasetMetadata;
use crate::optimized::multiscale::MultiscaleLevelSpec;
use crate::output::{MultiscaleEncoding, ReprojectionSpec};

use super::geometry_info::GeometryInfo;

#[derive(Debug, Clone)]
/// Stores resolved source, geometry, projection, extent, and encoding state for optimized output.
pub(super) struct ResolvedOptimization {
  source_metadata: SourceDatasetMetadata,
  geometry: GeometryInfo,
  reprojection: ReprojectionSpec,
  target_extent: Extent2D,
  levels: Vec<MultiscaleLevelSpec>,
  multiscale_encoding: MultiscaleEncoding,
}

impl ResolvedOptimization {
  pub(super) fn new(
    source_metadata: SourceDatasetMetadata,
    geometry: GeometryInfo,
    reprojection: ReprojectionSpec,
    target_extent: Extent2D,
    levels: Vec<MultiscaleLevelSpec>,
    multiscale_encoding: MultiscaleEncoding,
  ) -> Self {
    Self {
      source_metadata,
      geometry,
      reprojection,
      target_extent,
      levels,
      multiscale_encoding,
    }
  }

  pub(super) fn source_metadata(&self) -> &SourceDatasetMetadata {
    &self.source_metadata
  }

  pub(super) fn geometry(&self) -> &GeometryInfo {
    &self.geometry
  }

  pub(super) fn reprojection(&self) -> &ReprojectionSpec {
    &self.reprojection
  }

  pub(super) fn target_extent(&self) -> Extent2D {
    self.target_extent
  }

  pub(super) fn levels(&self) -> &[MultiscaleLevelSpec] {
    &self.levels
  }

  pub(super) fn multiscale_encoding(&self) -> MultiscaleEncoding {
    self.multiscale_encoding
  }

  pub(super) fn delta_binary_packed_column_paths(&self) -> Vec<String> {
    self.multiscale_encoding.delta_binary_packed_column_paths(
      &self.levels,
      self.geometry.ty,
      self.geometry.has_z,
      self.geometry.has_m,
    )
  }
}

//! Stores the complete resolved state for optimized output.

use crate::geometry::Extent2D;
use crate::input::SourceDatasetMetadata;
use crate::optimized::multiscale::GeometryEncoding;
use crate::output::{MultiscaleEncoding, ReprojectionSpec};

use super::geometry::OptimizedGeometry;

#[derive(Debug, Clone)]
/// Stores resolved source, geometry, projection, extent, and encoding state for optimized output.
pub(super) struct ResolvedOptimization {
  source_metadata: SourceDatasetMetadata,
  geometry: OptimizedGeometry,
  reprojection: ReprojectionSpec,
  target_extent: Extent2D,
  encodings: Vec<GeometryEncoding>,
  multiscale_encoding: MultiscaleEncoding,
}

impl ResolvedOptimization {
  pub(super) fn new(
    source_metadata: SourceDatasetMetadata,
    geometry: OptimizedGeometry,
    reprojection: ReprojectionSpec,
    target_extent: Extent2D,
    encodings: Vec<GeometryEncoding>,
    multiscale_encoding: MultiscaleEncoding,
  ) -> Self {
    Self {
      source_metadata,
      geometry,
      reprojection,
      target_extent,
      encodings,
      multiscale_encoding,
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

  pub(super) fn multiscale_encoding(&self) -> MultiscaleEncoding {
    self.multiscale_encoding
  }

  pub(super) fn delta_binary_packed_column_paths(&self) -> Vec<String> {
    match self.multiscale_encoding {
      MultiscaleEncoding::Pbf => Vec::new(),
      MultiscaleEncoding::QuantizedNative => {
        crate::optimized::multiscale::native_coordinate_column_paths(
          &self.encodings,
          self.geometry.geometry_type,
          self.geometry.has_z,
          self.geometry.has_m,
        )
      }
    }
  }
}

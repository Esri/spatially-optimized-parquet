//! Owns geometry classification and validation for spatially optimized output.

use crate::geometry::{GeometryColumn, GeometryFamily};
use crate::pipeline::ResolvedSpatialSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Groups optimized geometry types by their clustering strategy.
pub(crate) enum ClusteringFamily {
  /// Uses scalar x/y columns and Morton Z-order indexing.
  PointGeometry,
  /// Uses bounds, XZ-order indexing, and multiscale geometry payloads.
  ComplexGeometry,
}

#[derive(Debug, Clone, PartialEq)]
/// Represents geometry facts required by optimized clustering and encoding.
pub(crate) struct GeometryInfo {
  /// Identifies the selected source geometry column.
  pub(crate) geometry: GeometryColumn,
  /// Defines the geometry family used by metadata and encoders.
  pub(crate) family: GeometryFamily,
  /// Defines the clustering strategy family.
  pub(crate) clustering_family: ClusteringFamily,
  /// Indicates whether source metadata declares Z values.
  pub(crate) has_z: bool,
  /// Indicates whether source metadata declares M values.
  pub(crate) has_m: bool,
}

impl GeometryInfo {
  /// Resolve optimized geometry from normalized source geometry facts.
  pub(crate) fn resolve(source: &ResolvedSpatialSource) -> Self {
    let family = source.geometry_family;
    Self {
      geometry: source.geometry.clone(),
      family,
      clustering_family: ClusteringFamily::from_geometry_family(family),
      has_z: source.has_z,
      has_m: source.has_m,
    }
  }
}

impl ClusteringFamily {
  /// Resolve the clustering strategy from one normalized geometry family.
  fn from_geometry_family(geometry_family: GeometryFamily) -> Self {
    match geometry_family {
      GeometryFamily::Point => Self::PointGeometry,
      GeometryFamily::MultiPoint | GeometryFamily::Polyline | GeometryFamily::Polygon => {
        Self::ComplexGeometry
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::ClusteringFamily;
  use crate::geometry::GeometryFamily;

  #[test]
  fn resolves_canonical_geometry_types_into_clustering_families() {
    assert_eq!(
      ClusteringFamily::from_geometry_family(GeometryFamily::Polygon),
      ClusteringFamily::ComplexGeometry
    );
    assert_eq!(
      ClusteringFamily::from_geometry_family(GeometryFamily::Point),
      ClusteringFamily::PointGeometry
    );
    assert_eq!(
      ClusteringFamily::from_geometry_family(GeometryFamily::MultiPoint),
      ClusteringFamily::ComplexGeometry
    );
  }
}

//! Owns geometry classification and validation for spatially optimized output.

use anyhow::Result;

use crate::geometry::{GeometryColumn, GeometryType};
use crate::geoparquet::ResolvedGeoParquetSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Groups optimized geometry types by their clustering strategy.
pub(crate) enum ClusteringFamily {
  /// Uses scalar x/y columns and Morton Z-order indexing.
  PointGeometry,
  /// Uses bounds, XZ-order indexing, and multiscale geometry payloads.
  ComplexGeometry,
}

/// Resolve clustering behavior from the canonical geometry type.
pub(super) fn clustering_family(ty: GeometryType) -> ClusteringFamily {
  match ty {
    GeometryType::Point => ClusteringFamily::PointGeometry,
    GeometryType::MultiPoint | GeometryType::Polyline | GeometryType::Polygon => {
      ClusteringFamily::ComplexGeometry
    }
  }
}

#[derive(Debug, Clone, PartialEq)]
/// Stores geometry facts required by optimized clustering and encoding.
pub(crate) struct GeometryInfo {
  /// Stores the selected source geometry column.
  pub(crate) geometry: GeometryColumn,
  /// Stores the geometry type used by metadata and encoders.
  pub(crate) ty: GeometryType,
  /// Stores the clustering strategy family.
  pub(crate) clustering_family: ClusteringFamily,
  /// Indicates whether source metadata declares Z values.
  pub(crate) has_z: bool,
  /// Indicates whether source metadata declares M values.
  pub(crate) has_m: bool,
}

impl GeometryInfo {
  /// Resolve optimized geometry from normalized source geometry facts.
  pub(crate) fn resolve(source: &ResolvedGeoParquetSource) -> Result<Self> {
    let ty = source.geometry_type;
    Ok(Self {
      geometry: source.geometry.clone(),
      ty,
      clustering_family: clustering_family(ty),
      has_z: source.has_z,
      has_m: source.has_m,
    })
  }
}

#[cfg(test)]
mod tests {
  use super::{ClusteringFamily, clustering_family};
  use crate::geometry::GeometryType;

  #[test]
  fn resolves_canonical_geometry_types_into_clustering_families() {
    assert_eq!(
      clustering_family(GeometryType::Polygon),
      ClusteringFamily::ComplexGeometry
    );
    assert_eq!(
      clustering_family(GeometryType::Point),
      ClusteringFamily::PointGeometry
    );
  }
}

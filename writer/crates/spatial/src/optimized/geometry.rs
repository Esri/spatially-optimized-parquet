//! Owns geometry classification and validation for spatially optimized output.

use anyhow::{Result, bail};

use crate::geometry::{GeometryCategory, GeometryKind, GeometrySpec};
use crate::geoparquet::ResolvedGeoParquetSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Groups optimized geometry types by their clustering strategy.
pub(super) enum ClusteringFamily {
  /// Uses scalar x/y columns and Morton Z-order indexing.
  Point,
  /// Uses bounds, XZ-order indexing, and multiscale geometry payloads.
  NonPoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Identifies geometry types supported by spatial optimization.
pub(super) enum OptimizedGeometryType {
  /// Represents single-point features.
  Point,
  /// Represents multipoint features.
  MultiPoint,
  /// Represents line string and multi-line string features.
  Polyline,
  /// Represents polygon and multipolygon features.
  Polygon,
}

impl OptimizedGeometryType {
  /// Return the canonical geodisplay metadata label.
  pub(super) fn as_str(self) -> &'static str {
    match self {
      Self::Point => "point",
      Self::MultiPoint => "multipoint",
      Self::Polyline => "polyline",
      Self::Polygon => "polygon",
    }
  }

  /// Return the clustering strategy for this optimized geometry type.
  pub(super) fn clustering_family(self) -> ClusteringFamily {
    match self {
      Self::Point => ClusteringFamily::Point,
      Self::MultiPoint | Self::Polyline | Self::Polygon => ClusteringFamily::NonPoint,
    }
  }

  /// Return the generic processing category used by shared geometry mechanics.
  pub(super) fn category(self) -> GeometryCategory {
    match self.clustering_family() {
      ClusteringFamily::Point => GeometryCategory::Point,
      ClusteringFamily::NonPoint => GeometryCategory::NonPoint,
    }
  }

  /// Classify one source geometry kind for optimized output.
  fn from_kind(kind: GeometryKind) -> Result<Self> {
    match kind {
      GeometryKind::Point => Ok(Self::Point),
      GeometryKind::MultiPoint => Ok(Self::MultiPoint),
      GeometryKind::LineString | GeometryKind::MultiLineString => Ok(Self::Polyline),
      GeometryKind::Polygon | GeometryKind::MultiPolygon => Ok(Self::Polygon),
      GeometryKind::GeometryCollection | GeometryKind::Unknown => {
        bail!("unsupported optimized geometry kind: {kind:?}")
      }
    }
  }

  /// Classify source geometry kinds while rejecting mixed optimized types.
  fn from_kinds(kinds: &[GeometryKind]) -> Result<Self> {
    let mut geometry_type = None;
    for kind in kinds {
      merge_optimized_geometry_type(&mut geometry_type, *kind)?;
    }
    geometry_type.ok_or_else(|| anyhow::anyhow!("unable to determine optimized geometry type"))
  }
}

#[derive(Debug, Clone, PartialEq)]
/// Stores geometry facts required by optimized clustering and encoding.
pub(super) struct OptimizedGeometry {
  /// Stores the selected source geometry column.
  pub(super) geometry_spec: GeometrySpec,
  /// Stores the optimized geometry type used by metadata and encoders.
  pub(super) geometry_type: OptimizedGeometryType,
  /// Stores the clustering strategy family.
  pub(super) clustering_family: ClusteringFamily,
  /// Indicates whether source metadata declares Z ordinates.
  pub(super) has_z: bool,
  /// Indicates whether source metadata declares M ordinates.
  pub(super) has_m: bool,
}

impl OptimizedGeometry {
  /// Resolve optimized geometry from normalized source geometry facts.
  pub(super) fn resolve(source: &ResolvedGeoParquetSource) -> Result<Self> {
    let geometry_type = OptimizedGeometryType::from_kinds(&source.geometry_types)?;
    let geometry = Self {
      geometry_spec: source.geometry_spec.clone(),
      geometry_type,
      clustering_family: geometry_type.clustering_family(),
      has_z: source.has_z,
      has_m: source.has_m,
    };
    geometry.validate()?;
    Ok(geometry)
  }

  /// Validate geometry dimensions and categories implemented by optimized encoding.
  fn validate(&self) -> Result<()> {
    if self.has_z || self.has_m {
      bail!("optimized output does not yet support Z/M geometries")
    }
    if matches!(self.geometry_type, OptimizedGeometryType::MultiPoint) {
      bail!("optimized output does not yet support multipoint geometries")
    }
    Ok(())
  }
}

/// Merge one source kind into an observed optimized geometry type.
fn merge_optimized_geometry_type(
  observed_type: &mut Option<OptimizedGeometryType>,
  kind: GeometryKind,
) -> Result<OptimizedGeometryType> {
  let candidate = OptimizedGeometryType::from_kind(kind)?;
  if let Some(existing) = observed_type
    && *existing != candidate
  {
    bail!("mixed optimized geometry types are unsupported: saw {existing:?} and {candidate:?}");
  }
  *observed_type = Some(candidate);
  Ok(candidate)
}

#[cfg(test)]
mod tests {
  use super::{ClusteringFamily, OptimizedGeometryType, merge_optimized_geometry_type};
  use crate::geometry::GeometryKind;

  #[test]
  fn classifies_source_kinds_into_optimized_types() {
    assert_eq!(
      OptimizedGeometryType::from_kind(GeometryKind::MultiLineString).unwrap(),
      OptimizedGeometryType::Polyline
    );
    assert_eq!(
      OptimizedGeometryType::from_kind(GeometryKind::MultiPolygon)
        .unwrap()
        .clustering_family(),
      ClusteringFamily::NonPoint
    );
  }

  #[test]
  fn rejects_mixed_optimized_types() {
    let error =
      OptimizedGeometryType::from_kinds(&[GeometryKind::Point, GeometryKind::Polygon]).unwrap_err();

    assert!(error.to_string().contains("mixed optimized geometry types"));
  }

  #[test]
  fn rejects_unsupported_source_kinds() {
    let error = OptimizedGeometryType::from_kind(GeometryKind::GeometryCollection).unwrap_err();

    assert!(
      error
        .to_string()
        .contains("unsupported optimized geometry kind")
    );
  }

  #[test]
  fn merges_repeated_compatible_kinds() {
    let mut observed_type = None;
    merge_optimized_geometry_type(&mut observed_type, GeometryKind::LineString).unwrap();
    merge_optimized_geometry_type(&mut observed_type, GeometryKind::MultiLineString).unwrap();

    assert_eq!(observed_type, Some(OptimizedGeometryType::Polyline));
  }
}

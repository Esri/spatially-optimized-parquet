//! Defines the format-neutral geometry vocabulary shared by codecs and spatial processing.
//!
//! [`GeometryType`] classifies a geometry into the representations used by the pipeline. It
//! deliberately groups line strings with multi-line strings and polygons with multipolygons,
//! because those pairs share one optimized and quantized layout. [`GeometryKind`] retains the
//! concrete WKB type when a codec must distinguish their binary framing.

use serde::{Deserialize, Serialize};

use super::{GeometryError, GeometryKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
/// Identifies the normalized geometry representation used by processing and codecs.
pub(crate) enum GeometryType {
  /// Represents single-point features.
  Point,
  /// Represents multipoint features.
  MultiPoint,
  /// Represents line string and multi-line string features.
  Polyline,
  /// Represents polygon and multipolygon features.
  Polygon,
}

impl GeometryType {
  /// Classify one concrete geometry kind.
  pub(crate) fn from_kind(kind: GeometryKind) -> Result<Self, GeometryError> {
    match kind {
      GeometryKind::Point => Ok(Self::Point),
      GeometryKind::MultiPoint => Ok(Self::MultiPoint),
      GeometryKind::LineString | GeometryKind::MultiLineString => Ok(Self::Polyline),
      GeometryKind::Polygon | GeometryKind::MultiPolygon => Ok(Self::Polygon),
      GeometryKind::GeometryCollection | GeometryKind::Unknown => Err(
        GeometryError::InvalidGeometry(format!("unsupported geometry kind: {kind:?}")),
      ),
    }
  }

  /// Classify source geometry kinds while rejecting mixed geometry families.
  pub(crate) fn from_kinds(kinds: &[GeometryKind]) -> Result<Self, GeometryError> {
    let mut ty = None;
    for kind in kinds {
      let next = Self::from_kind(*kind)?;
      if ty.is_some_and(|current| current != next) {
        return Err(GeometryError::InvalidGeometry(format!(
          "mixed geometry families are not supported: {kinds:?}; \
             Polygon/MultiPolygon and LineString/MultiLineString are compatible"
        )));
      }
      ty = Some(next);
    }
    ty.ok_or_else(|| {
      GeometryError::InvalidGeometry("unable to determine geometry type".to_string())
    })
  }

  /// Return the canonical geodisplay metadata label.
  pub(crate) fn as_str(self) -> &'static str {
    match self {
      Self::Point => "point",
      Self::MultiPoint => "multipoint",
      Self::Polyline => "polyline",
      Self::Polygon => "polygon",
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq)]
/// Represents one coordinate in the format-neutral geometry model.
pub(crate) struct Coord {
  /// Defines the horizontal x coordinate.
  pub(crate) x: f64,
  /// Defines the horizontal y coordinate.
  pub(crate) y: f64,
  /// Provides the optional vertical coordinate.
  pub(crate) z: Option<f64>,
  /// Provides the optional measure coordinate.
  pub(crate) m: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
/// Represents one format-neutral geometry as coordinate parts.
///
/// Each `lengths` entry owns the following contiguous coordinate range. Multipart and polygon
/// boundaries therefore survive decoding without making codecs depend on a source geometry crate.
pub(crate) struct Geometry {
  /// Defines the normalized geometry representation.
  pub(crate) ty: GeometryType,
  /// Provides coordinates in part order.
  pub(crate) coordinates: Vec<Coord>,
  /// Defines the coordinate count for each part.
  pub(crate) lengths: Vec<u32>,
}

impl Geometry {
  /// Build geometry from ordered coordinates and matching part lengths.
  pub(crate) fn new(
    ty: GeometryType,
    coordinates: Vec<Coord>,
    lengths: Vec<u32>,
  ) -> Result<Self, GeometryError> {
    let coordinate_count = lengths
      .iter()
      .try_fold(0usize, |count, length| count.checked_add(*length as usize))
      .ok_or_else(|| {
        GeometryError::InvalidGeometry("geometry coordinate count overflow".to_string())
      })?;
    if coordinate_count != coordinates.len() {
      return Err(GeometryError::InvalidGeometry(format!(
        "geometry part lengths total {coordinate_count}, but geometry has {} coordinates",
        coordinates.len()
      )));
    }
    Ok(Self {
      ty,
      coordinates,
      lengths,
    })
  }
}

#[cfg(test)]
mod tests {
  use super::{super::GeometryKind, Coord, Geometry, GeometryType};

  #[test]
  fn classifies_concrete_kinds_into_geometry_types() {
    assert_eq!(
      GeometryType::from_kind(GeometryKind::MultiLineString).unwrap(),
      GeometryType::Polyline
    );
    assert_eq!(
      GeometryType::from_kinds(&[GeometryKind::Polygon, GeometryKind::MultiPolygon]).unwrap(),
      GeometryType::Polygon
    );
    assert_eq!(
      GeometryType::from_kinds(&[GeometryKind::LineString, GeometryKind::MultiLineString,])
        .unwrap(),
      GeometryType::Polyline
    );
  }

  #[test]
  fn rejects_mixed_geometry_families() {
    let error =
      GeometryType::from_kinds(&[GeometryKind::Point, GeometryKind::Polygon]).unwrap_err();

    assert!(error.to_string().contains("mixed geometry families"));
    assert!(error.to_string().contains("Point"));
    assert!(error.to_string().contains("Polygon"));
  }

  #[test]
  fn rejects_part_lengths_that_do_not_cover_coordinates() {
    let error = Geometry::new(
      GeometryType::Polyline,
      vec![Coord {
        x: 0.0,
        y: 0.0,
        z: None,
        m: None,
      }],
      vec![2],
    )
    .unwrap_err();

    assert!(error.to_string().contains("part lengths"));
  }
}

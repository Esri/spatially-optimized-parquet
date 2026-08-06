//! Defines the format-neutral geometry vocabulary shared by codecs and spatial processing.
//!
//! [`GeometryFamily`] classifies a geometry into the representations used by the pipeline. It
//! deliberately groups line strings with multi-line strings and polygons with multipolygons,
//! because those pairs share one optimized and quantized layout. [`GeometryType`] retains the
//! concrete WKB type when a codec must distinguish their binary framing.

use serde::{Deserialize, Serialize};

use super::{GeometryError, GeometryType};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
/// Identifies the normalized geometry representation used by processing and codecs.
pub(crate) enum GeometryFamily {
  /// Represents single-point features.
  Point,
  /// Represents multipoint features.
  MultiPoint,
  /// Represents line string and multi-line string features.
  Polyline,
  /// Represents polygon and multipolygon features.
  Polygon,
}

impl GeometryFamily {
  /// Classify one concrete geometry kind.
  pub(crate) fn from_type(geometry_type: GeometryType) -> Result<Self, GeometryError> {
    match geometry_type {
      GeometryType::Point => Ok(Self::Point),
      GeometryType::MultiPoint => Ok(Self::MultiPoint),
      GeometryType::LineString | GeometryType::MultiLineString => Ok(Self::Polyline),
      GeometryType::Polygon | GeometryType::MultiPolygon => Ok(Self::Polygon),
      GeometryType::GeometryCollection | GeometryType::Unknown => Err(
        GeometryError::InvalidGeometry(format!("unsupported geometry type: {geometry_type:?}")),
      ),
    }
  }

  /// Classify source geometry types while rejecting mixed geometry families.
  pub(crate) fn from_types(geometry_types: &[GeometryType]) -> Result<Self, GeometryError> {
    let mut ty = None;
    for geometry_type in geometry_types {
      let next = Self::from_type(*geometry_type)?;
      if ty.is_some_and(|current| current != next) {
        return Err(GeometryError::InvalidGeometry(format!(
          "mixed geometry families are not supported: {geometry_types:?}; \
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
  /// Defines the concrete WKB geometry type.
  pub(crate) ty: GeometryType,
  /// Provides coordinates in part order.
  pub(crate) coordinates: Vec<Coord>,
  /// Defines the coordinate count for each part.
  pub(crate) lengths: Vec<u32>,
  /// Groups polygon rings by polygon for Polygon and MultiPolygon WKB output.
  pub(crate) polygon_ring_counts: Vec<u32>,
}

impl Geometry {
  /// Build geometry from ordered coordinates, parts, and polygon topology.
  pub(crate) fn new(
    ty: GeometryType,
    coordinates: Vec<Coord>,
    lengths: Vec<u32>,
    polygon_ring_counts: Vec<u32>,
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
    let polygon_ring_count = polygon_ring_counts
      .iter()
      .try_fold(0usize, |count, ring_count| {
        count.checked_add(*ring_count as usize)
      })
      .ok_or_else(|| GeometryError::InvalidGeometry("polygon ring count overflow".to_string()))?;
    if ty.family()? == GeometryFamily::Polygon && polygon_ring_count != lengths.len() {
      return Err(GeometryError::InvalidGeometry(format!(
        "polygon ring counts total {polygon_ring_count}, but geometry has {} parts",
        lengths.len()
      )));
    }
    if ty.family()? != GeometryFamily::Polygon && !polygon_ring_counts.is_empty() {
      return Err(GeometryError::InvalidGeometry(
        "non-polygon geometry cannot define polygon ring counts".to_string(),
      ));
    }
    Ok(Self {
      ty,
      coordinates,
      lengths,
      polygon_ring_counts,
    })
  }
}

impl GeometryType {
  /// Resolve the normalized family used by shared processing and output layout.
  pub(crate) fn family(self) -> Result<GeometryFamily, GeometryError> {
    GeometryFamily::from_type(self)
  }
}

#[cfg(test)]
mod tests {
  use super::{super::GeometryType, Coord, Geometry, GeometryFamily};

  #[test]
  fn classifies_concrete_kinds_into_geometry_types() {
    assert_eq!(
      GeometryFamily::from_type(GeometryType::MultiLineString).unwrap(),
      GeometryFamily::Polyline
    );
    assert_eq!(
      GeometryFamily::from_types(&[GeometryType::Polygon, GeometryType::MultiPolygon]).unwrap(),
      GeometryFamily::Polygon
    );
    assert_eq!(
      GeometryFamily::from_types(&[GeometryType::LineString, GeometryType::MultiLineString,])
        .unwrap(),
      GeometryFamily::Polyline
    );
  }

  #[test]
  fn rejects_mixed_geometry_families() {
    let error =
      GeometryFamily::from_types(&[GeometryType::Point, GeometryType::Polygon]).unwrap_err();

    assert!(error.to_string().contains("mixed geometry families"));
    assert!(error.to_string().contains("Point"));
    assert!(error.to_string().contains("Polygon"));
  }

  #[test]
  fn rejects_part_lengths_that_do_not_cover_coordinates() {
    let error = Geometry::new(
      GeometryType::LineString,
      vec![Coord {
        x: 0.0,
        y: 0.0,
        z: None,
        m: None,
      }],
      vec![2],
      Vec::new(),
    )
    .unwrap_err();

    assert!(error.to_string().contains("part lengths"));
  }
}

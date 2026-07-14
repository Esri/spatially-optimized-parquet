use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Groups geometry shapes by point-specific or general geometry processing.
pub(crate) enum GeometryCategory {
  /// Uses direct point coordinate extraction.
  Point,
  /// Uses general geometry bounds extraction.
  NonPoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Normalizes concrete geometry kinds into format-independent shape groups.
pub(crate) enum GeometryShape {
  /// Represents single-point features.
  Point,
  /// Represents multipoint features.
  MultiPoint,
  /// Represents line string and multi-line string features.
  Polyline,
  /// Represents polygon and multipolygon features.
  Polygon,
}

impl GeometryShape {
  /// Classify one concrete source geometry kind.
  pub(crate) fn from_kind(kind: GeometryKind) -> Result<Self> {
    match kind {
      GeometryKind::Point => Ok(Self::Point),
      GeometryKind::MultiPoint => Ok(Self::MultiPoint),
      GeometryKind::LineString | GeometryKind::MultiLineString => Ok(Self::Polyline),
      GeometryKind::Polygon | GeometryKind::MultiPolygon => Ok(Self::Polygon),
      GeometryKind::GeometryCollection | GeometryKind::Unknown => {
        anyhow::bail!("unsupported geometry kind: {kind:?}")
      }
    }
  }

  /// Classify source geometry kinds while rejecting mixed shape groups.
  pub(crate) fn from_kinds(kinds: &[GeometryKind]) -> Result<Self> {
    let mut shape = None;
    for kind in kinds {
      let next = Self::from_kind(*kind)?;
      if shape.is_some_and(|current| current != next) {
        anyhow::bail!("mixed geometry shapes are not supported");
      }
      shape = Some(next);
    }
    shape.ok_or_else(|| anyhow::anyhow!("unable to determine geometry shape"))
  }

  /// Return the processing category shared by output mechanics.
  pub(crate) fn category(self) -> GeometryCategory {
    match self {
      Self::Point => GeometryCategory::Point,
      Self::MultiPoint | Self::Polyline | Self::Polygon => GeometryCategory::NonPoint,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Identifies the concrete geometry shape represented by source metadata or WKB.
pub(crate) enum GeometryKind {
  /// Represents one point.
  Point,
  /// Represents one line string.
  LineString,
  /// Represents multiple points.
  MultiPoint,
  /// Represents multiple line strings.
  MultiLineString,
  /// Represents one polygon.
  Polygon,
  /// Represents multiple polygons.
  MultiPolygon,
  /// Represents a heterogeneous geometry collection.
  GeometryCollection,
  /// Represents a geometry whose concrete type cannot be established.
  Unknown,
}

#[derive(Debug, Clone, PartialEq)]
/// Describes the selected geometry column and its source encoding.
pub(crate) struct GeometrySpec {
  /// Stores the Arrow column containing geometry values.
  pub(crate) column: String,
  /// Stores the physical encoding used by that column.
  pub(crate) encoding: GeometryEncoding,
  /// Stores the known source geometry kind when metadata can determine it.
  pub(crate) geometry_kind: Option<GeometryKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Identifies supported physical geometry encodings.
pub(crate) enum GeometryEncoding {
  /// Represents Open Geospatial Consortium Well-Known Binary.
  Wkb,
}

/// Map a decoded WKB geometry type into the repository geometry taxonomy.
pub(crate) fn geometry_kind_from_wkb_type(
  geometry_type: wkb::reader::GeometryType,
) -> GeometryKind {
  match geometry_type {
    wkb::reader::GeometryType::Point => GeometryKind::Point,
    wkb::reader::GeometryType::LineString => GeometryKind::LineString,
    wkb::reader::GeometryType::MultiPoint => GeometryKind::MultiPoint,
    wkb::reader::GeometryType::MultiLineString => GeometryKind::MultiLineString,
    wkb::reader::GeometryType::Polygon => GeometryKind::Polygon,
    wkb::reader::GeometryType::MultiPolygon => GeometryKind::MultiPolygon,
    wkb::reader::GeometryType::GeometryCollection => GeometryKind::GeometryCollection,
    _ => GeometryKind::Unknown,
  }
}

/// Decode WKB and return its geometry kind.
pub(crate) fn geometry_kind_from_wkb(bytes: &[u8]) -> Result<GeometryKind> {
  let wkb_geom = wkb::reader::read_wkb(bytes)?;
  Ok(geometry_kind_from_wkb_type(wkb_geom.geometry_type()))
}

#[cfg(test)]
mod tests {
  use geo::polygon;
  use wkb::writer::write_geometry;

  use super::{GeometryKind, geometry_kind_from_wkb};

  fn encoded_geometry(geometry: &geo::Geometry) -> Vec<u8> {
    let mut bytes = Vec::new();
    write_geometry(&mut bytes, geometry, &Default::default()).unwrap();
    bytes
  }

  #[test]
  fn wkb_kind_decodes_point() {
    let bytes = encoded_geometry(&geo::Geometry::Point(geo::Point::new(1.0, 2.0)));

    assert_eq!(geometry_kind_from_wkb(&bytes).unwrap(), GeometryKind::Point);
  }

  #[test]
  fn wkb_kind_decodes_polygon() {
    let geometry = geo::Geometry::Polygon(geo::polygon![
        (x: 0.0, y: 0.0),
        (x: 1.0, y: 0.0),
        (x: 1.0, y: 1.0),
        (x: 0.0, y: 1.0),
        (x: 0.0, y: 0.0),
    ]);
    let bytes = encoded_geometry(&geometry);

    assert_eq!(
      geometry_kind_from_wkb(&bytes).unwrap(),
      GeometryKind::Polygon
    );
  }

  #[test]
  fn wkb_kind_decodes_geometry_collection() {
    let geometry = geo::Geometry::GeometryCollection(geo::GeometryCollection::new_from(vec![
      geo::Geometry::Point(geo::Point::new(0.0, 0.0)),
      geo::Geometry::Point(geo::Point::new(1.0, 1.0)),
    ]));
    let bytes = encoded_geometry(&geometry);

    assert_eq!(
      geometry_kind_from_wkb(&bytes).unwrap(),
      GeometryKind::GeometryCollection
    );
  }
}

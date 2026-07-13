//! Defines the smallest format-independent vocabulary shared by the spatial pipeline.
//!
//! [`GeometryKind`] normalizes geometry declarations from GDAL, GeoParquet metadata, and WKB.
//! [`GeometryEncoding`] describes the physical representation stored in Arrow, while
//! [`GeometrySpec`] binds that representation to the selected source column. Input providers
//! produce these values and analysis/output code consumes them without depending on the source
//! format.
//!
//! WKB decoding helpers intentionally determine only the top-level geometry kind. Coordinate
//! traversal, extents, reprojection, and display encoding belong to dedicated modules.

use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Identifies the concrete geometry shape represented by source metadata or WKB.
pub enum GeometryKind {
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
pub struct GeometrySpec {
  /// Stores the Arrow column containing geometry values.
  pub column: String,
  /// Stores the physical encoding used by that column.
  pub encoding: GeometryEncoding,
  /// Stores the known source geometry kind when metadata can determine it.
  pub geometry_kind: Option<GeometryKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Identifies supported physical geometry encodings.
pub enum GeometryEncoding {
  /// Represents Open Geospatial Consortium Well-Known Binary.
  Wkb,
}

/// Map a decoded WKB geometry type into the repository geometry taxonomy.
pub fn geometry_kind_from_wkb_type(geometry_type: wkb::reader::GeometryType) -> GeometryKind {
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
pub fn geometry_kind_from_wkb(bytes: &[u8]) -> Result<GeometryKind> {
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

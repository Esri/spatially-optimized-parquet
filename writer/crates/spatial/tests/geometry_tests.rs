use spatial::geometry::{GeometryEncoding, GeometryKind, GeometrySpec, geometry_kind_from_wkb};

use geo::polygon;
use wkb::writer::write_geometry;

#[test]
fn geometry_kind_from_wkb_point() {
  let geom = geo::Geometry::Point(geo::Point::new(1.0, 2.0));
  let mut buf = Vec::new();
  write_geometry(&mut buf, &geom, &Default::default()).unwrap();
  let kind = geometry_kind_from_wkb(&buf).unwrap();
  assert_eq!(kind, GeometryKind::Point);
}

#[test]
fn geometry_kind_from_wkb_polygon() {
  let geom = geo::Geometry::Polygon(geo::polygon![
      (x: 0.0, y: 0.0),
      (x: 1.0, y: 0.0),
      (x: 1.0, y: 1.0),
      (x: 0.0, y: 1.0),
      (x: 0.0, y: 0.0),
  ]);
  let mut buf = Vec::new();
  write_geometry(&mut buf, &geom, &Default::default()).unwrap();
  let kind = geometry_kind_from_wkb(&buf).unwrap();
  assert_eq!(kind, GeometryKind::Polygon);
}

#[test]
fn geometry_kind_from_wkb_geometry_collection() {
  let geom = geo::Geometry::GeometryCollection(geo::GeometryCollection::new_from(vec![
    geo::Geometry::Point(geo::Point::new(0.0, 0.0)),
    geo::Geometry::Point(geo::Point::new(1.0, 1.0)),
  ]));
  let mut buf = Vec::new();
  write_geometry(&mut buf, &geom, &Default::default()).unwrap();
  let kind = geometry_kind_from_wkb(&buf).unwrap();
  assert_eq!(kind, GeometryKind::GeometryCollection);
}

#[test]
fn geometry_spec_holds_explicit_kind() {
  let spec = GeometrySpec {
    column: "geometry".to_string(),
    encoding: GeometryEncoding::Wkb,
    geometry_kind: Some(GeometryKind::Polygon),
  };
  assert_eq!(spec.geometry_kind, Some(GeometryKind::Polygon));
}

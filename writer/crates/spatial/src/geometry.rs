use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeometryKind {
  Point,
  LineString,
  MultiPoint,
  MultiLineString,
  Polygon,
  MultiPolygon,
  GeometryCollection,
  Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeometrySpec {
  pub column: String,
  pub encoding: GeometryEncoding,
  pub geometry_kind: Option<GeometryKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeometryEncoding {
  Wkb,
}

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

pub fn geometry_kind_from_wkb(bytes: &[u8]) -> Result<GeometryKind> {
  let wkb_geom = wkb::reader::read_wkb(bytes)?;
  Ok(geometry_kind_from_wkb_type(wkb_geom.geometry_type()))
}

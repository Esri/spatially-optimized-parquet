//! Traverses supported geometry structures through a shared part sink.

use anyhow::{Context, Result, bail};
use geo_traits::Dimensions;
#[cfg(test)]
use geo_traits::{
  CoordTrait, GeometryTrait, GeometryType as GeoGeometryType, LineStringTrait,
  MultiLineStringTrait, MultiPointTrait, MultiPolygonTrait, PointTrait, PolygonTrait,
};

use crate::geometry::{
  Extent2D, GeometryKind, GeometryType, PolygonRingOrder, WkbCoordinate, WkbHeader,
  visit_wkb_geometry as decode_wkb_geometry,
};

pub(crate) use crate::geometry::{WkbPartRole as GeometryPartRole, WkbSink as GeometryPartSink};

impl Extent2D {
  /// Calculate the axis-aligned extent of one decoded WKB geometry.
  pub(crate) fn from_wkb(bytes: &[u8]) -> Result<Self> {
    let mut collector = BoundsCollector::default();
    decode_wkb_geometry(bytes, PolygonRingOrder::Preserve, &mut collector)?;
    collector
      .finish()
      .context("geometry missing bounding rectangle")
  }
}

impl WkbHeader {
  /// Visit one WKB geometry through canonical part traversal.
  pub(crate) fn visit<S: GeometryPartSink>(bytes: &[u8], sink: &mut S) -> Result<Self> {
    decode_wkb_geometry(bytes, PolygonRingOrder::Preserve, sink)
  }
}

impl GeometryType {
  /// Visit one display geometry while enforcing this optimized geometry type.
  pub(crate) fn visit_wkb_for_display<S: GeometryPartSink>(
    self,
    bytes: &[u8],
    sink: &mut S,
  ) -> Result<Dimensions> {
    let header = decode_wkb_geometry(bytes, PolygonRingOrder::Reverse, sink)?;
    let kind_matches = match self {
      Self::Point => header.kind == GeometryKind::Point,
      Self::MultiPoint => header.kind == GeometryKind::MultiPoint,
      Self::Polyline => {
        matches!(
          header.kind,
          GeometryKind::LineString | GeometryKind::MultiLineString
        )
      }
      Self::Polygon => {
        matches!(
          header.kind,
          GeometryKind::Polygon | GeometryKind::MultiPolygon
        )
      }
    };
    if !kind_matches {
      bail!(
        "WKB geometry {:?} does not match optimized type {self:?}",
        header.kind
      );
    }
    Ok(header.dimensions)
  }
}

#[cfg(test)]
pub(super) fn visit_geometry_for_display<G: GeometryTrait<T = f64>, S: GeometryPartSink>(
  geometry: &G,
  geometry_type: GeometryType,
  sink: &mut S,
) -> Result<()> {
  match (geometry_type, geometry.as_type()) {
    (GeometryType::Point, GeoGeometryType::Point(point)) => visit_point(point, sink),
    (GeometryType::MultiPoint, GeoGeometryType::MultiPoint(points)) => {
      visit_multipoint(points, sink)
    }
    (GeometryType::Polyline, GeoGeometryType::LineString(line)) => {
      visit_line_string(line, GeometryPartRole::Other, sink)
    }
    (GeometryType::Polyline, GeoGeometryType::MultiLineString(lines)) => {
      visit_multiline_string(lines, sink)
    }
    (GeometryType::Polygon, GeoGeometryType::Polygon(polygon)) => {
      visit_polygon_reversed(polygon, sink)
    }
    (GeometryType::Polygon, GeoGeometryType::MultiPolygon(polygons)) => {
      visit_multipolygon_reversed(polygons, sink)
    }
    _ => bail!("unsupported geometry for optimized type {geometry_type:?}"),
  }
  Ok(())
}

#[cfg(test)]
fn visit_point<P: PointTrait<T = f64>, S: GeometryPartSink>(point: &P, sink: &mut S) {
  sink.start_part(GeometryPartRole::Other);
  if let Some(coord) = point.coord() {
    let (x, y) = coord.x_y();
    sink.push_coord(WkbCoordinate {
      x,
      y,
      z: None,
      m: None,
    });
  }
  sink.finish_part();
}

#[cfg(test)]
fn visit_multipoint<MP: MultiPointTrait<T = f64>, S: GeometryPartSink>(points: &MP, sink: &mut S) {
  sink.start_part(GeometryPartRole::Other);
  for point in points.points() {
    if let Some(coord) = point.coord() {
      let (x, y) = coord.x_y();
      sink.push_coord(WkbCoordinate {
        x,
        y,
        z: None,
        m: None,
      });
    }
  }
  sink.finish_part();
}

#[cfg(test)]
fn visit_multiline_string<ML: MultiLineStringTrait<T = f64>, S: GeometryPartSink>(
  lines: &ML,
  sink: &mut S,
) {
  for line in lines.line_strings() {
    visit_line_string(&line, GeometryPartRole::Other, sink);
  }
}

#[cfg(test)]
fn visit_multipolygon_reversed<MP: MultiPolygonTrait<T = f64>, S: GeometryPartSink>(
  polygons: &MP,
  sink: &mut S,
) {
  for polygon in polygons.polygons() {
    visit_polygon_reversed(&polygon, sink);
  }
}

#[cfg(test)]
fn visit_polygon_reversed<P: PolygonTrait<T = f64>, S: GeometryPartSink>(
  polygon: &P,
  sink: &mut S,
) {
  if let Some(exterior) = polygon.exterior() {
    visit_line_string_reversed(&exterior, GeometryPartRole::Exterior, sink);
  } else {
    sink.start_part(GeometryPartRole::Exterior);
    sink.finish_part();
  }
  for interior in polygon.interiors() {
    visit_line_string_reversed(&interior, GeometryPartRole::Interior, sink);
  }
}

#[cfg(test)]
fn visit_line_string<L: LineStringTrait<T = f64>, S: GeometryPartSink>(
  line: &L,
  role: GeometryPartRole,
  sink: &mut S,
) {
  sink.start_part(role);
  for coord in line.coords() {
    let (x, y) = coord.x_y();
    sink.push_coord(WkbCoordinate {
      x,
      y,
      z: None,
      m: None,
    });
  }
  sink.finish_part();
}

#[cfg(test)]
fn visit_line_string_reversed<L: LineStringTrait<T = f64>, S: GeometryPartSink>(
  line: &L,
  role: GeometryPartRole,
  sink: &mut S,
) {
  let coordinates: Vec<_> = line.coords().map(|coordinate| coordinate.x_y()).collect();
  sink.start_part(role);
  for (x, y) in coordinates.into_iter().rev() {
    sink.push_coord(WkbCoordinate {
      x,
      y,
      z: None,
      m: None,
    });
  }
  sink.finish_part();
}

#[derive(Default)]
pub(super) struct ExtentAccumulator {
  extent: Option<Extent2D>,
}

impl ExtentAccumulator {
  pub(super) fn push(&mut self, x: f64, y: f64) {
    match self.extent.as_mut() {
      Some(extent) => {
        extent.xmin = extent.xmin.min(x);
        extent.ymin = extent.ymin.min(y);
        extent.xmax = extent.xmax.max(x);
        extent.ymax = extent.ymax.max(y);
      }
      None => {
        self.extent = Some(Extent2D {
          xmin: x,
          ymin: y,
          xmax: x,
          ymax: y,
        });
      }
    }
  }

  pub(super) fn finish(self) -> Option<Extent2D> {
    self.extent
  }
}

#[derive(Default)]
struct BoundsCollector {
  bounds: ExtentAccumulator,
}

impl BoundsCollector {
  fn finish(self) -> Option<Extent2D> {
    self.bounds.finish()
  }
}

impl GeometryPartSink for BoundsCollector {
  fn start_part(&mut self, _: GeometryPartRole) {}

  fn push_coord(&mut self, coordinate: WkbCoordinate) {
    self.bounds.push(coordinate.x, coordinate.y);
  }

  fn finish_part(&mut self) {}
}

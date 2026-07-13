//! Traverses supported geometry structures through a shared part sink.

use anyhow::{Context, Result, bail};
use geo_traits::{
  CoordTrait, GeometryTrait, GeometryType, LineStringTrait, MultiLineStringTrait, MultiPointTrait,
  MultiPolygonTrait, PointTrait, PolygonTrait,
};

use crate::analysis::{DisplayGeometryType, Extent2D};

/// Decode a WKB point and return its x/y coordinate.
pub fn point_xy_from_wkb(bytes: &[u8]) -> Result<(f64, f64)> {
  let geometry = wkb::reader::read_wkb(bytes)?;
  point_xy_from_geometry_trait(&geometry)
}

/// Decode WKB and calculate its axis-aligned extent.
pub fn geometry_extent_from_wkb(bytes: &[u8]) -> Result<Extent2D> {
  let geometry = wkb::reader::read_wkb(bytes)?;
  geometry_extent_from_trait(&geometry).context("geometry missing bounding rectangle")
}

pub(crate) fn geometry_extent_from_trait<G: GeometryTrait<T = f64>>(
  geometry: &G,
) -> Option<Extent2D> {
  let mut collector = BoundsCollector::default();
  match geometry.as_type() {
    GeometryType::Point(point) => visit_point(point, &mut collector),
    GeometryType::LineString(line) => visit_line_string(line, &mut collector),
    GeometryType::Polygon(polygon) => visit_polygon(polygon, &mut collector),
    GeometryType::MultiPoint(points) => visit_multipoint(points, &mut collector),
    GeometryType::MultiLineString(lines) => visit_multiline_string(lines, &mut collector),
    GeometryType::MultiPolygon(polygons) => visit_multipolygon(polygons, &mut collector),
    _ => return None,
  }
  collector.finish()
}

pub(super) fn visit_geometry_for_display<G: GeometryTrait<T = f64>, S: GeometryPartSink>(
  geometry: &G,
  geometry_type: DisplayGeometryType,
  sink: &mut S,
) -> Result<()> {
  match (geometry_type, geometry.as_type()) {
    (DisplayGeometryType::Point, GeometryType::Point(point)) => visit_point(point, sink),
    (DisplayGeometryType::MultiPoint, GeometryType::MultiPoint(points)) => {
      visit_multipoint(points, sink)
    }
    (DisplayGeometryType::Polyline, GeometryType::LineString(line)) => {
      visit_line_string(line, sink)
    }
    (DisplayGeometryType::Polyline, GeometryType::MultiLineString(lines)) => {
      visit_multiline_string(lines, sink)
    }
    (DisplayGeometryType::Polygon, GeometryType::Polygon(polygon)) => visit_polygon(polygon, sink),
    (DisplayGeometryType::Polygon, GeometryType::MultiPolygon(polygons)) => {
      visit_multipolygon(polygons, sink)
    }
    _ => bail!("unsupported geometry for display type {geometry_type:?}"),
  }
  Ok(())
}

fn point_xy_from_geometry_trait<G: GeometryTrait<T = f64>>(geometry: &G) -> Result<(f64, f64)> {
  match geometry.as_type() {
    GeometryType::Point(point) => point
      .coord()
      .map(|coord| coord.x_y())
      .context("point missing coordinate"),
    _ => bail!("expected point geometry"),
  }
}

fn visit_point<P: PointTrait<T = f64>, S: GeometryPartSink>(point: &P, sink: &mut S) {
  sink.start_part();
  if let Some(coord) = point.coord() {
    let (x, y) = coord.x_y();
    sink.push_coord(x, y);
  }
  sink.finish_part();
}

fn visit_multipoint<MP: MultiPointTrait<T = f64>, S: GeometryPartSink>(points: &MP, sink: &mut S) {
  sink.start_part();
  for point in points.points() {
    if let Some(coord) = point.coord() {
      let (x, y) = coord.x_y();
      sink.push_coord(x, y);
    }
  }
  sink.finish_part();
}

fn visit_multiline_string<ML: MultiLineStringTrait<T = f64>, S: GeometryPartSink>(
  lines: &ML,
  sink: &mut S,
) {
  for line in lines.line_strings() {
    visit_line_string(&line, sink);
  }
}

fn visit_multipolygon<MP: MultiPolygonTrait<T = f64>, S: GeometryPartSink>(
  polygons: &MP,
  sink: &mut S,
) {
  for polygon in polygons.polygons() {
    visit_polygon(&polygon, sink);
  }
}

fn visit_polygon<P: PolygonTrait<T = f64>, S: GeometryPartSink>(polygon: &P, sink: &mut S) {
  if let Some(exterior) = polygon.exterior() {
    visit_line_string(&exterior, sink);
  } else {
    sink.start_part();
    sink.finish_part();
  }
  for interior in polygon.interiors() {
    visit_line_string(&interior, sink);
  }
}

fn visit_line_string<L: LineStringTrait<T = f64>, S: GeometryPartSink>(line: &L, sink: &mut S) {
  sink.start_part();
  for coord in line.coords() {
    let (x, y) = coord.x_y();
    sink.push_coord(x, y);
  }
  sink.finish_part();
}

pub(super) trait GeometryPartSink {
  fn start_part(&mut self);
  fn push_coord(&mut self, x: f64, y: f64);
  fn finish_part(&mut self);
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
  fn start_part(&mut self) {}

  fn push_coord(&mut self, x: f64, y: f64) {
    self.bounds.push(x, y);
  }

  fn finish_part(&mut self) {}
}

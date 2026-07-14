//! Traverses supported geometry structures through a shared part sink.

use anyhow::{Context, Result, bail};
use geo_traits::{
  CoordTrait, Dimensions, GeometryTrait, GeometryType, LineStringTrait, MultiLineStringTrait,
  MultiPointTrait, MultiPolygonTrait, PointTrait, PolygonTrait,
};

use crate::geometry::{Extent2D, GeometryKind, geometry_kind_from_wkb_type};
use crate::optimized::OptimizedGeometryType;

/// Decode a WKB point and return its x/y coordinate.
pub(crate) fn point_xy_from_wkb(bytes: &[u8]) -> Result<(f64, f64)> {
  let geometry = wkb::reader::read_wkb(bytes)?;
  point_xy_from_geometry_trait(&geometry)
}

/// Decode WKB and calculate its axis-aligned extent.
pub(crate) fn geometry_extent_from_wkb(bytes: &[u8]) -> Result<Extent2D> {
  let geometry = wkb::reader::read_wkb(bytes)?;
  geometry_extent_from_trait(&geometry).context("geometry missing bounding rectangle")
}

/// Visit one decoded WKB geometry through the canonical part traversal.
pub(crate) fn visit_wkb_geometry<S: GeometryPartSink>(
  bytes: &[u8],
  sink: &mut S,
) -> Result<(GeometryKind, Dimensions)> {
  let geometry = wkb::reader::read_wkb(bytes)?;
  let kind = geometry_kind_from_wkb_type(geometry.geometry_type());
  let dimensions = geometry.dim();
  visit_geometry_parts(&geometry, sink)?;
  Ok((kind, dimensions))
}

fn visit_geometry_parts<G: GeometryTrait<T = f64>, S: GeometryPartSink>(
  geometry: &G,
  sink: &mut S,
) -> Result<()> {
  match geometry.as_type() {
    GeometryType::Point(point) => visit_point(point, sink),
    GeometryType::LineString(line) => visit_line_string(line, GeometryPartRole::Other, sink),
    GeometryType::Polygon(polygon) => visit_polygon(polygon, sink),
    GeometryType::MultiPoint(points) => visit_multipoint(points, sink),
    GeometryType::MultiLineString(lines) => visit_multiline_string(lines, sink),
    GeometryType::MultiPolygon(polygons) => visit_multipolygon(polygons, sink),
    GeometryType::GeometryCollection(_) => {
      bail!("geometry collections are not supported by display optimization")
    }
    _ => bail!("unsupported WKB geometry"),
  }
  Ok(())
}

fn geometry_extent_from_trait<G: GeometryTrait<T = f64>>(geometry: &G) -> Option<Extent2D> {
  let mut collector = BoundsCollector::default();
  match geometry.as_type() {
    GeometryType::Point(point) => visit_point(point, &mut collector),
    GeometryType::LineString(line) => {
      visit_line_string(line, GeometryPartRole::Other, &mut collector)
    }
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
  geometry_type: OptimizedGeometryType,
  sink: &mut S,
) -> Result<()> {
  match (geometry_type, geometry.as_type()) {
    (OptimizedGeometryType::Point, GeometryType::Point(point)) => visit_point(point, sink),
    (OptimizedGeometryType::MultiPoint, GeometryType::MultiPoint(points)) => {
      visit_multipoint(points, sink)
    }
    (OptimizedGeometryType::Polyline, GeometryType::LineString(line)) => {
      visit_line_string(line, GeometryPartRole::Other, sink)
    }
    (OptimizedGeometryType::Polyline, GeometryType::MultiLineString(lines)) => {
      visit_multiline_string(lines, sink)
    }
    (OptimizedGeometryType::Polygon, GeometryType::Polygon(polygon)) => {
      visit_polygon(polygon, sink)
    }
    (OptimizedGeometryType::Polygon, GeometryType::MultiPolygon(polygons)) => {
      visit_multipolygon(polygons, sink)
    }
    _ => bail!("unsupported geometry for optimized type {geometry_type:?}"),
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
  sink.start_part(GeometryPartRole::Other);
  if let Some(coord) = point.coord() {
    let (x, y) = coord.x_y();
    sink.push_coord(x, y);
  }
  sink.finish_part();
}

fn visit_multipoint<MP: MultiPointTrait<T = f64>, S: GeometryPartSink>(points: &MP, sink: &mut S) {
  sink.start_part(GeometryPartRole::Other);
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
    visit_line_string(&line, GeometryPartRole::Other, sink);
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
    visit_line_string(&exterior, GeometryPartRole::Exterior, sink);
  } else {
    sink.start_part(GeometryPartRole::Exterior);
    sink.finish_part();
  }
  for interior in polygon.interiors() {
    visit_line_string(&interior, GeometryPartRole::Interior, sink);
  }
}

fn visit_line_string<L: LineStringTrait<T = f64>, S: GeometryPartSink>(
  line: &L,
  role: GeometryPartRole,
  sink: &mut S,
) {
  sink.start_part(role);
  for coord in line.coords() {
    let (x, y) = coord.x_y();
    sink.push_coord(x, y);
  }
  sink.finish_part();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GeometryPartRole {
  Exterior,
  Interior,
  Other,
}

pub(crate) trait GeometryPartSink {
  fn start_part(&mut self, role: GeometryPartRole);
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
  fn start_part(&mut self, _: GeometryPartRole) {}

  fn push_coord(&mut self, x: f64, y: f64) {
    self.bounds.push(x, y);
  }

  fn finish_part(&mut self) {}
}

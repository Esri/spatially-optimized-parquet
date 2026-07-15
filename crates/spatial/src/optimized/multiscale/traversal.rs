//! Traverses supported geometry structures through a shared part sink.

use anyhow::{Context, Result, bail};
use geo_traits::Dimensions;
#[cfg(test)]
use geo_traits::{
  CoordTrait, GeometryTrait, GeometryType, LineStringTrait, MultiLineStringTrait, MultiPointTrait,
  MultiPolygonTrait, PointTrait, PolygonTrait,
};

use crate::geometry::{
  Extent2D, GeometryKind, PolygonRingOrder, read_wkb_point,
  visit_wkb_geometry as decode_wkb_geometry,
};
use crate::optimized::OptimizedGeometryType;

pub(crate) use crate::geometry::{WkbPartRole as GeometryPartRole, WkbSink as GeometryPartSink};

/// Decode a WKB point and return its x/y coordinate.
pub(crate) fn point_xy_from_wkb(bytes: &[u8]) -> Result<(f64, f64)> {
  read_wkb_point(bytes)
}

/// Decode WKB and calculate its axis-aligned extent.
pub(crate) fn geometry_extent_from_wkb(bytes: &[u8]) -> Result<Extent2D> {
  let mut collector = BoundsCollector::default();
  decode_wkb_geometry(bytes, PolygonRingOrder::Preserve, &mut collector)?;
  collector
    .finish()
    .context("geometry missing bounding rectangle")
}

/// Visit one decoded WKB geometry through the canonical part traversal.
pub(crate) fn visit_wkb_geometry<S: GeometryPartSink>(
  bytes: &[u8],
  sink: &mut S,
) -> Result<(GeometryKind, Dimensions)> {
  let header = decode_wkb_geometry(bytes, PolygonRingOrder::Preserve, sink)?;
  Ok((header.kind, header.dimensions))
}

pub(super) fn visit_wkb_geometry_for_display<S: GeometryPartSink>(
  bytes: &[u8],
  geometry_type: OptimizedGeometryType,
  sink: &mut S,
) -> Result<()> {
  let header = decode_wkb_geometry(bytes, PolygonRingOrder::Reverse, sink)?;
  let kind_matches = match geometry_type {
    OptimizedGeometryType::Point => header.kind == GeometryKind::Point,
    OptimizedGeometryType::MultiPoint => header.kind == GeometryKind::MultiPoint,
    OptimizedGeometryType::Polyline => {
      matches!(
        header.kind,
        GeometryKind::LineString | GeometryKind::MultiLineString
      )
    }
    OptimizedGeometryType::Polygon => {
      matches!(
        header.kind,
        GeometryKind::Polygon | GeometryKind::MultiPolygon
      )
    }
  };
  if !kind_matches {
    bail!(
      "WKB geometry {:?} does not match optimized type {geometry_type:?}",
      header.kind
    );
  }
  Ok(())
}

#[cfg(test)]
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
      visit_polygon_reversed(polygon, sink)
    }
    (OptimizedGeometryType::Polygon, GeometryType::MultiPolygon(polygons)) => {
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
    sink.push_coord(x, y);
  }
  sink.finish_part();
}

#[cfg(test)]
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
    sink.push_coord(x, y);
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
    sink.push_coord(x, y);
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

  fn push_coord(&mut self, x: f64, y: f64) {
    self.bounds.push(x, y);
  }

  fn finish_part(&mut self) {}
}

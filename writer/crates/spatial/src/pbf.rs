//! Converts source geometry into compact multiscale display payloads.
//!
//! A shared `geo_traits` traversal flattens supported points, lines, and polygons into
//! interleaved coordinate sequences plus part lengths. The same traversal can accumulate bounds
//! without materializing payload coordinates. Encoding then quantizes coordinates, delta-encodes
//! them, removes redundant collinear vertices without violating geometry minimums, and serializes
//! the result with the module's internal protocol-buffer wire schema.
//!
//! Geometry decoding and flattening happen independently of level-specific quantization, allowing
//! one flat payload to feed every multiscale level. [`GeometryEncodeScratch`] reuses vectors and
//! serialization storage across rows and levels, which reduces allocation pressure in the
//! non-point UDF and direct batch encoder.

use anyhow::{Context, Result, bail};
use geo_traits::{
  CoordTrait, GeometryTrait, GeometryType, LineStringTrait, MultiLineStringTrait, MultiPointTrait,
  MultiPolygonTrait, PointTrait, PolygonTrait,
};
use geo_types::Geometry;
use prost::Message;

use crate::analysis::{DisplayGeometryType, Extent2D};
use crate::multiscale::GeometryEncoding;

#[derive(Debug, Clone, PartialEq)]
/// Stores flattened coordinate and part-length sequences without computed bounds.
pub struct FlatGeometryPayload {
  /// Stores interleaved x/y coordinates for every traversed part.
  pub coords: Vec<f64>,
  /// Stores the coordinate-pair count of each geometry part.
  pub lengths: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq)]
/// Stores flattened geometry sequences together with their source extent.
pub struct GeometryPayload {
  /// Stores interleaved x/y coordinates for every traversed part.
  pub coords: Vec<f64>,
  /// Stores the coordinate-pair count of each geometry part.
  pub lengths: Vec<u32>,
  /// Stores the extent observed while flattening coordinates.
  pub bounds: Extent2D,
}

#[derive(Debug, Default)]
/// Reuses quantization vectors and serialization storage across geometry encodes.
pub struct GeometryEncodeScratch {
  quantized_coords: Vec<i64>,
  quantized_lengths: Vec<u32>,
  buffer: Vec<u8>,
}

#[derive(Clone, PartialEq, Message)]
/// Defines the compact protocol-buffer wire payload for one display geometry.
struct PbfGeometry {
  #[prost(uint32, repeated, tag = "2")]
  lengths: Vec<u32>,
  #[prost(sint64, repeated, tag = "3")]
  coords: Vec<i64>,
}

/// Decode WKB into flattened display geometry and bounds.
pub fn geometry_payload_from_wkb(
  bytes: &[u8],
  geometry_type: DisplayGeometryType,
) -> Result<GeometryPayload> {
  let (payload, bounds) = geometry_payload_parts_from_wkb(bytes, geometry_type, true)?;
  Ok(GeometryPayload {
    coords: payload.coords,
    lengths: payload.lengths,
    bounds: bounds.unwrap_or_default(),
  })
}

/// Decode WKB into flattened display geometry without calculating bounds.
pub fn flat_geometry_payload_from_wkb(
  bytes: &[u8],
  geometry_type: DisplayGeometryType,
) -> Result<FlatGeometryPayload> {
  Ok(geometry_payload_parts_from_wkb(bytes, geometry_type, false)?.0)
}

/// Flatten an owned `geo_types` geometry and calculate its bounds.
pub fn geometry_payload_from_geometry(
  geometry: &Geometry<f64>,
  geometry_type: DisplayGeometryType,
) -> Result<GeometryPayload> {
  let (payload, bounds) = geometry_payload_parts_from_geometry(geometry, geometry_type, true)?;
  Ok(GeometryPayload {
    coords: payload.coords,
    lengths: payload.lengths,
    bounds: bounds.unwrap_or_default(),
  })
}

/// Quantize and encode a complete geometry payload into a new byte buffer.
pub fn encode_geometry(payload: &GeometryPayload, encoding: &GeometryEncoding) -> Result<Vec<u8>> {
  let mut scratch = GeometryEncodeScratch::default();
  encode_geometry_owned_with_scratch_impl(&payload.coords, &payload.lengths, encoding, &mut scratch)
}

/// Quantize and encode a flat payload into reusable scratch storage.
///
/// The returned slice remains valid until the scratch value is mutated again.
pub fn encode_flat_geometry_with_scratch<'a>(
  payload: &FlatGeometryPayload,
  encoding: &GeometryEncoding,
  scratch: &'a mut GeometryEncodeScratch,
) -> Result<&'a [u8]> {
  encode_geometry_with_scratch_impl(&payload.coords, &payload.lengths, encoding, scratch)
}

/// Quantize with reusable scratch vectors and return an owned encoded buffer.
pub fn encode_flat_geometry_owned_with_scratch(
  payload: &FlatGeometryPayload,
  encoding: &GeometryEncoding,
  scratch: &mut GeometryEncodeScratch,
) -> Result<Vec<u8>> {
  encode_geometry_owned_with_scratch_impl(&payload.coords, &payload.lengths, encoding, scratch)
}

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

#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
struct EncodedPayload {
  coords: Vec<i64>,
  lengths: Vec<u32>,
}

/// Traverse a geometry implementation and calculate its supported two-dimensional extent.
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

fn geometry_payload_parts_from_wkb(
  bytes: &[u8],
  geometry_type: DisplayGeometryType,
  track_bounds: bool,
) -> Result<(FlatGeometryPayload, Option<Extent2D>)> {
  let geometry = wkb::reader::read_wkb(bytes)?;
  geometry_payload_parts_from_geometry_trait(&geometry, geometry_type, track_bounds)
}

fn geometry_payload_parts_from_geometry(
  geometry: &Geometry<f64>,
  geometry_type: DisplayGeometryType,
  track_bounds: bool,
) -> Result<(FlatGeometryPayload, Option<Extent2D>)> {
  geometry_payload_parts_from_geometry_trait(geometry, geometry_type, track_bounds)
}

fn geometry_payload_parts_from_geometry_trait<G: GeometryTrait<T = f64>>(
  geometry: &G,
  geometry_type: DisplayGeometryType,
  track_bounds: bool,
) -> Result<(FlatGeometryPayload, Option<Extent2D>)> {
  let mut builder = PayloadBuilder::new(track_bounds);
  visit_geometry_for_display(geometry, geometry_type, &mut builder)?;
  Ok(builder.finish())
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

/// Traverse only geometry structures compatible with the requested display category.
fn visit_geometry_for_display<G: GeometryTrait<T = f64>, S: GeometryPartSink>(
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

/// Receives normalized geometry parts from the shared traversal algorithm.
trait GeometryPartSink {
  fn start_part(&mut self);
  fn push_coord(&mut self, x: f64, y: f64);
  fn finish_part(&mut self);
}

#[derive(Default)]
/// Accumulates coordinate extrema without building a display payload.
struct ExtentAccumulator {
  extent: Option<Extent2D>,
}

impl ExtentAccumulator {
  fn push(&mut self, x: f64, y: f64) {
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

  fn finish(self) -> Option<Extent2D> {
    self.extent
  }
}

/// Builds flattened coordinates, part lengths, and optional bounds during traversal.
struct PayloadBuilder {
  coords: Vec<f64>,
  lengths: Vec<u32>,
  current_len: u32,
  bounds: Option<ExtentAccumulator>,
}

impl PayloadBuilder {
  fn new(track_bounds: bool) -> Self {
    Self {
      coords: Vec::new(),
      lengths: Vec::new(),
      current_len: 0,
      bounds: track_bounds.then(ExtentAccumulator::default),
    }
  }

  fn finish(self) -> (FlatGeometryPayload, Option<Extent2D>) {
    (
      FlatGeometryPayload {
        coords: self.coords,
        lengths: self.lengths,
      },
      self.bounds.and_then(ExtentAccumulator::finish),
    )
  }
}

impl GeometryPartSink for PayloadBuilder {
  fn start_part(&mut self) {
    self.current_len = 0;
  }

  fn push_coord(&mut self, x: f64, y: f64) {
    self.coords.push(x);
    self.coords.push(y);
    self.current_len += 1;
    if let Some(bounds) = self.bounds.as_mut() {
      bounds.push(x, y);
    }
  }

  fn finish_part(&mut self) {
    self.lengths.push(self.current_len);
    self.current_len = 0;
  }
}

#[derive(Default)]
/// Adapts coordinate traversal into an extent accumulator.
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

#[cfg(test)]
/// Quantize delta-encoded coordinates, remove redundant collinear vertices, and serialize PBF.
fn encode_quantized_payload(
  payload: &GeometryPayload,
  encoding: &GeometryEncoding,
) -> Result<EncodedPayload> {
  let mut coords = Vec::with_capacity(payload.coords.len());
  let mut lengths = Vec::with_capacity(payload.lengths.len());
  encode_quantized_payload_into(
    &payload.coords,
    &payload.lengths,
    encoding,
    &mut coords,
    &mut lengths,
  )?;
  Ok(EncodedPayload { coords, lengths })
}

/// Encode quantized coordinates into caller-provided reusable storage.
fn encode_quantized_payload_into(
  input_coords: &[f64],
  input_lengths: &[u32],
  encoding: &GeometryEncoding,
  coords: &mut Vec<i64>,
  lengths: &mut Vec<u32>,
) -> Result<()> {
  coords.clear();
  lengths.clear();
  let mut offset = 0usize;

  for &length in input_lengths {
    let point_count = length as usize;
    if point_count == 0 {
      continue;
    }

    let ring_start_coord_index = coords.len();
    let part = &input_coords[offset..offset + point_count * 2];
    let mut vertices = part.chunks_exact(2);
    let first = vertices
      .next()
      .expect("non-empty part should have a first vertex");
    let mut prev_x = quantize(
      first[0],
      encoding.transform.scale[0],
      encoding.transform.translate[0],
    )?;
    let mut prev_y = quantize(
      first[1],
      encoding.transform.scale[1],
      encoding.transform.translate[1],
    )?;
    coords.push(prev_x);
    coords.push(prev_y);
    let mut out_length = 1u32;
    let mut prev_dx = 0i64;
    let mut prev_dy = 0i64;

    for vertex in vertices {
      let x = quantize(
        vertex[0],
        encoding.transform.scale[0],
        encoding.transform.translate[0],
      )?;
      let y = quantize(
        vertex[1],
        encoding.transform.scale[1],
        encoding.transform.translate[1],
      )?;

      if x == prev_x && y == prev_y {
        continue;
      }

      let dx = x - prev_x;
      let dy = y - prev_y;
      if is_collinear_delta(prev_dx, prev_dy, dx, dy) {
        let len = coords.len();
        coords[len - 2] += dx;
        coords[len - 1] += dy;
        prev_x += dx;
        prev_y += dy;
      } else {
        coords.push(dx);
        coords.push(dy);
        prev_x = x;
        prev_y = y;
        prev_dx = dx;
        prev_dy = dy;
        out_length += 1;
      }
    }

    if out_length < encoding.min_length as u32 {
      coords.truncate(ring_start_coord_index);
    } else {
      lengths.push(out_length);
    }

    offset += point_count * 2;
  }

  Ok(())
}

/// Encode borrowed coordinate slices and return scratch-owned bytes.
fn encode_geometry_with_scratch_impl<'a>(
  coords: &[f64],
  lengths: &[u32],
  encoding: &GeometryEncoding,
  scratch: &'a mut GeometryEncodeScratch,
) -> Result<&'a [u8]> {
  let message = quantized_message_from_slices(coords, lengths, encoding, scratch)?;
  scratch.buffer.clear();
  scratch.buffer.reserve(message.encoded_len());
  message.encode(&mut scratch.buffer)?;
  scratch.quantized_coords = message.coords;
  scratch.quantized_lengths = message.lengths;
  Ok(scratch.buffer.as_slice())
}

/// Encode borrowed coordinate slices and copy the result into an owned buffer.
fn encode_geometry_owned_with_scratch_impl(
  coords: &[f64],
  lengths: &[u32],
  encoding: &GeometryEncoding,
  scratch: &mut GeometryEncodeScratch,
) -> Result<Vec<u8>> {
  let message = quantized_message_from_slices(coords, lengths, encoding, scratch)?;
  let mut buffer = Vec::with_capacity(message.encoded_len());
  message.encode(&mut buffer)?;
  scratch.quantized_coords = message.coords;
  scratch.quantized_lengths = message.lengths;
  Ok(buffer)
}

fn quantized_message_from_slices(
  coords: &[f64],
  lengths: &[u32],
  encoding: &GeometryEncoding,
  scratch: &mut GeometryEncodeScratch,
) -> Result<PbfGeometry> {
  let mut message = PbfGeometry {
    lengths: std::mem::take(&mut scratch.quantized_lengths),
    coords: std::mem::take(&mut scratch.quantized_coords),
  };
  encode_quantized_payload_into(
    coords,
    lengths,
    encoding,
    &mut message.coords,
    &mut message.lengths,
  )?;
  Ok(message)
}

/// Quantize one coordinate and reject non-finite or overflowing results.
fn quantize(value: f64, scale: f64, translate: f64) -> Result<i64> {
  let normalized = ((value - translate) / scale).round();
  if !normalized.is_finite() || normalized < i64::MIN as f64 || normalized > i64::MAX as f64 {
    bail!("quantized coordinate out of range")
  }
  Ok(normalized as i64)
}

/// Return the minimum vertices retained for each display geometry type.
pub fn min_vertex_count(geometry_type: DisplayGeometryType) -> usize {
  match geometry_type {
    DisplayGeometryType::MultiPoint => 1,
    DisplayGeometryType::Polyline => 2,
    DisplayGeometryType::Polygon => 3,
    DisplayGeometryType::Point => 1,
  }
}

fn is_collinear_delta(prev_dx: i64, prev_dy: i64, dx: i64, dy: i64) -> bool {
  prev_dx * dy == dx * prev_dy && (prev_dx * dx + prev_dy * dy) > 0
}

#[cfg(test)]
mod tests {
  use std::io::Cursor;

  use geo_types::{Geometry, line_string, polygon};

  use super::*;
  use crate::metadata::output::QuantizationTransform;

  #[derive(Clone, PartialEq, Message)]
  struct EsriPbfGeometry {
    #[prost(uint32, repeated, tag = "2")]
    lengths: Vec<u32>,
    #[prost(sint64, repeated, tag = "3")]
    coords: Vec<i64>,
  }

  #[test]
  fn quantized_encoding_merges_collinear_lines() {
    let geometry =
      Geometry::LineString(line_string![(x: 0.0, y: 0.0), (x: 1.0, y: 1.0), (x: 2.0, y: 2.0)]);
    let payload = geometry_payload_from_geometry(&geometry, DisplayGeometryType::Polyline).unwrap();
    let encoding = GeometryEncoding {
      level: 0,
      column: "level_0".to_string(),
      resolution: 1.0,
      scale: 1.0,
      transform: QuantizationTransform {
        scale: [1.0, 1.0, 1.0, 1.0],
        translate: [0.0, 0.0, 0.0, 0.0],
      },
      min_length: 2,
    };

    let encoded = encode_quantized_payload(&payload, &encoding).unwrap();
    assert_eq!(encoded.lengths, vec![2]);
    assert_eq!(encoded.coords, vec![0, 0, 2, 2]);
  }

  #[test]
  fn quantized_encoding_drops_duplicate_snapped_vertices() {
    let geometry = Geometry::LineString(
      line_string![(x: 0.1, y: 0.1), (x: 0.2, y: 0.2), (x: 0.9, y: 0.9), (x: 1.0, y: 1.0)],
    );
    let payload = geometry_payload_from_geometry(&geometry, DisplayGeometryType::Polyline).unwrap();
    let encoding = GeometryEncoding {
      level: 0,
      column: "level_0".to_string(),
      resolution: 1.0,
      scale: 1.0,
      transform: QuantizationTransform {
        scale: [1.0, 1.0, 1.0, 1.0],
        translate: [0.0, 0.0, 0.0, 0.0],
      },
      min_length: 2,
    };

    let encoded = encode_quantized_payload(&payload, &encoding).unwrap();
    assert_eq!(encoded.lengths, vec![2]);
    assert_eq!(encoded.coords, vec![0, 0, 1, 1]);
  }

  #[test]
  fn quantized_encoding_drops_parts_below_min_length() {
    let geometry = Geometry::LineString(line_string![(x: 0.1, y: 0.1), (x: 0.2, y: 0.2)]);
    let payload = geometry_payload_from_geometry(&geometry, DisplayGeometryType::Polyline).unwrap();
    let encoding = GeometryEncoding {
      level: 0,
      column: "level_0".to_string(),
      resolution: 1.0,
      scale: 1.0,
      transform: QuantizationTransform {
        scale: [1.0, 1.0, 1.0, 1.0],
        translate: [0.0, 0.0, 0.0, 0.0],
      },
      min_length: 2,
    };

    let encoded = encode_quantized_payload(&payload, &encoding).unwrap();
    assert!(encoded.lengths.is_empty());
    assert!(encoded.coords.is_empty());
  }

  #[test]
  fn encodes_polygon_pbf() {
    let geometry = Geometry::Polygon(polygon![
        (x: 0.0, y: 0.0),
        (x: 1.0, y: 0.0),
        (x: 1.0, y: 1.0),
        (x: 0.0, y: 0.0),
    ]);
    let payload = geometry_payload_from_geometry(&geometry, DisplayGeometryType::Polygon).unwrap();
    let encoding = GeometryEncoding {
      level: 0,
      column: "level_0".to_string(),
      resolution: 1.0,
      scale: 1.0,
      transform: QuantizationTransform {
        scale: [1.0, 1.0, 1.0, 1.0],
        translate: [0.0, 0.0, 0.0, 0.0],
      },
      min_length: 3,
    };

    let bytes = encode_geometry(&payload, &encoding).unwrap();
    let decoded = PbfGeometry::decode(Cursor::new(&bytes)).unwrap();
    let esri_decoded = EsriPbfGeometry::decode(Cursor::new(bytes)).unwrap();
    assert_eq!(decoded.lengths, vec![4]);
    assert_eq!(decoded.coords.len(), 8);
    assert_eq!(esri_decoded.lengths, vec![4]);
    assert_eq!(esri_decoded.coords.len(), 8);
  }

  #[test]
  fn wkb_payload_matches_geometry_payload() {
    let geometry = Geometry::Polygon(polygon![
        (x: 0.0, y: 0.0),
        (x: 2.0, y: 0.0),
        (x: 2.0, y: 1.0),
        (x: 0.0, y: 0.0),
    ]);
    let mut buffer = Vec::new();
    wkb::writer::write_geometry(
      &mut buffer,
      &geometry,
      &wkb::writer::WriteOptions::default(),
    )
    .unwrap();

    let from_geometry =
      geometry_payload_from_geometry(&geometry, DisplayGeometryType::Polygon).unwrap();
    let from_wkb = geometry_payload_from_wkb(&buffer, DisplayGeometryType::Polygon).unwrap();
    let flat = flat_geometry_payload_from_wkb(&buffer, DisplayGeometryType::Polygon).unwrap();

    assert_eq!(from_wkb, from_geometry);
    assert_eq!(flat.coords, from_geometry.coords);
    assert_eq!(flat.lengths, from_geometry.lengths);
  }
}

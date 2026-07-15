//! Decodes ISO WKB directly into repository geometry visitors.

use anyhow::{Result, bail};
use geo_traits::Dimensions;

use super::GeometryKind;

const EWKB_Z: u32 = 0x8000_0000;
const EWKB_M: u32 = 0x4000_0000;
const EWKB_SRID: u32 = 0x2000_0000;
const EWKB_BBOX: u32 = 0x1000_0000;
const EWKB_TYPE_MASK: u32 = 0x0fff_ffff;
const MAX_NESTING_DEPTH: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WkbPartRole {
  Exterior,
  Interior,
  Other,
}

pub(crate) trait WkbSink {
  fn start_part(&mut self, role: WkbPartRole);
  fn push_coord(&mut self, x: f64, y: f64);
  fn finish_part(&mut self);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PolygonRingOrder {
  Preserve,
  Reverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WkbHeader {
  pub(crate) kind: GeometryKind,
  pub(crate) dimensions: Dimensions,
}

#[derive(Debug, Clone, Copy)]
enum ByteOrder {
  BigEndian,
  LittleEndian,
}

#[derive(Debug, Clone, Copy)]
struct ParsedHeader {
  public: WkbHeader,
  byte_order: ByteOrder,
  has_z: bool,
  has_m: bool,
}

impl ParsedHeader {
  fn coordinate_width(self) -> usize {
    2 + usize::from(self.has_z) + usize::from(self.has_m)
  }
}

pub(crate) fn read_wkb_header(bytes: &[u8]) -> Result<WkbHeader> {
  let mut reader = WkbReader::new(bytes);
  Ok(reader.read_header()?.public)
}

pub(crate) fn geometry_kind_from_wkb(bytes: &[u8]) -> Result<GeometryKind> {
  Ok(read_wkb_header(bytes)?.kind)
}

pub(crate) fn read_wkb_point(bytes: &[u8]) -> Result<(f64, f64)> {
  let mut reader = WkbReader::new(bytes);
  let header = reader.read_header()?;
  if header.public.kind != GeometryKind::Point {
    bail!("expected point WKB, found {:?}", header.public.kind);
  }
  reader.read_coordinate(header)
}

pub(crate) fn visit_wkb_geometry(
  bytes: &[u8],
  polygon_ring_order: PolygonRingOrder,
  sink: &mut impl WkbSink,
) -> Result<WkbHeader> {
  let mut reader = WkbReader::new(bytes);
  let header = reader.read_geometry(polygon_ring_order, sink, 0)?;
  if reader.remaining() != 0 {
    bail!("WKB contains {} trailing bytes", reader.remaining());
  }
  Ok(header)
}

struct WkbReader<'a> {
  bytes: &'a [u8],
  offset: usize,
}

impl<'a> WkbReader<'a> {
  fn new(bytes: &'a [u8]) -> Self {
    Self { bytes, offset: 0 }
  }

  fn remaining(&self) -> usize {
    self.bytes.len().saturating_sub(self.offset)
  }

  fn read_geometry(
    &mut self,
    polygon_ring_order: PolygonRingOrder,
    sink: &mut impl WkbSink,
    depth: usize,
  ) -> Result<WkbHeader> {
    if depth > MAX_NESTING_DEPTH {
      bail!("WKB nesting exceeds {MAX_NESTING_DEPTH} levels");
    }
    let header = self.read_header()?;
    match header.public.kind {
      GeometryKind::Point => self.read_point_part(header, sink)?,
      GeometryKind::LineString => self.read_line_string(header, WkbPartRole::Other, sink)?,
      GeometryKind::Polygon => self.read_polygon(header, polygon_ring_order, sink)?,
      GeometryKind::MultiPoint => self.read_multi_point(header, sink)?,
      GeometryKind::MultiLineString => self.read_multi_line_string(header, sink)?,
      GeometryKind::MultiPolygon => {
        self.read_multi_polygon(header, polygon_ring_order, sink, depth + 1)?
      }
      GeometryKind::GeometryCollection => {
        self.read_geometry_collection(header, polygon_ring_order, sink, depth + 1)?
      }
      GeometryKind::Unknown => bail!("unsupported WKB geometry type"),
    }
    Ok(header.public)
  }

  fn read_header(&mut self) -> Result<ParsedHeader> {
    let byte_order = match self.read_u8()? {
      0 => ByteOrder::BigEndian,
      1 => ByteOrder::LittleEndian,
      value => bail!("invalid WKB byte order {value}"),
    };
    let encoded_type = self.read_u32(byte_order)?;
    let (base_type, has_z, has_m, has_srid) = decode_type(encoded_type)?;
    if has_srid {
      self.read_u32(byte_order)?;
    }
    Ok(ParsedHeader {
      public: WkbHeader {
        kind: decode_geometry_kind(base_type),
        dimensions: dimensions(has_z, has_m),
      },
      byte_order,
      has_z,
      has_m,
    })
  }

  fn read_point_part(&mut self, header: ParsedHeader, sink: &mut impl WkbSink) -> Result<()> {
    sink.start_part(WkbPartRole::Other);
    let (x, y) = self.read_coordinate(header)?;
    sink.push_coord(x, y);
    sink.finish_part();
    Ok(())
  }

  fn read_line_string(
    &mut self,
    header: ParsedHeader,
    role: WkbPartRole,
    sink: &mut impl WkbSink,
  ) -> Result<()> {
    let point_count = self.read_count(header.byte_order, "point")?;
    sink.start_part(role);
    for _ in 0..point_count {
      let (x, y) = self.read_coordinate(header)?;
      sink.push_coord(x, y);
    }
    sink.finish_part();
    Ok(())
  }

  fn read_reversed_ring(
    &mut self,
    header: ParsedHeader,
    role: WkbPartRole,
    sink: &mut impl WkbSink,
  ) -> Result<()> {
    let point_count = self.read_count(header.byte_order, "point")?;
    let coordinate_width = header
      .coordinate_width()
      .checked_mul(size_of::<f64>())
      .ok_or_else(|| anyhow::anyhow!("WKB coordinate width overflows usize"))?;
    let coordinate_bytes = point_count
      .checked_mul(coordinate_width)
      .ok_or_else(|| anyhow::anyhow!("WKB ring byte length overflows usize"))?;
    let ring_start = self.offset;
    let ring_end = ring_start
      .checked_add(coordinate_bytes)
      .ok_or_else(|| anyhow::anyhow!("WKB ring end offset overflows usize"))?;
    self.require_end(ring_end)?;

    sink.start_part(role);
    for point_index in (0..point_count).rev() {
      let point_offset = ring_start + point_index * coordinate_width;
      let x = self.read_f64_at(point_offset, header.byte_order)?;
      let y = self.read_f64_at(point_offset + size_of::<f64>(), header.byte_order)?;
      sink.push_coord(x, y);
    }
    sink.finish_part();
    self.offset = ring_end;
    Ok(())
  }

  fn read_polygon(
    &mut self,
    header: ParsedHeader,
    polygon_ring_order: PolygonRingOrder,
    sink: &mut impl WkbSink,
  ) -> Result<()> {
    let ring_count = self.read_count(header.byte_order, "ring")?;
    for ring_index in 0..ring_count {
      let role = if ring_index == 0 {
        WkbPartRole::Exterior
      } else {
        WkbPartRole::Interior
      };
      match polygon_ring_order {
        PolygonRingOrder::Preserve => self.read_line_string(header, role, sink)?,
        PolygonRingOrder::Reverse => self.read_reversed_ring(header, role, sink)?,
      }
    }
    Ok(())
  }

  fn read_multi_point(&mut self, parent: ParsedHeader, sink: &mut impl WkbSink) -> Result<()> {
    let count = self.read_count(parent.byte_order, "point")?;
    sink.start_part(WkbPartRole::Other);
    for _ in 0..count {
      let header = self.read_header()?;
      require_kind(header.public.kind, GeometryKind::Point, "MultiPoint member")?;
      let (x, y) = self.read_coordinate(header)?;
      sink.push_coord(x, y);
    }
    sink.finish_part();
    Ok(())
  }

  fn read_multi_line_string(
    &mut self,
    parent: ParsedHeader,
    sink: &mut impl WkbSink,
  ) -> Result<()> {
    let count = self.read_count(parent.byte_order, "line string")?;
    for _ in 0..count {
      let header = self.read_header()?;
      require_kind(
        header.public.kind,
        GeometryKind::LineString,
        "MultiLineString member",
      )?;
      self.read_line_string(header, WkbPartRole::Other, sink)?;
    }
    Ok(())
  }

  fn read_multi_polygon(
    &mut self,
    parent: ParsedHeader,
    polygon_ring_order: PolygonRingOrder,
    sink: &mut impl WkbSink,
    _: usize,
  ) -> Result<()> {
    let count = self.read_count(parent.byte_order, "polygon")?;
    for _ in 0..count {
      let header = self.read_header()?;
      require_kind(
        header.public.kind,
        GeometryKind::Polygon,
        "MultiPolygon member",
      )?;
      self.read_polygon(header, polygon_ring_order, sink)?;
    }
    Ok(())
  }

  fn read_geometry_collection(
    &mut self,
    parent: ParsedHeader,
    polygon_ring_order: PolygonRingOrder,
    sink: &mut impl WkbSink,
    depth: usize,
  ) -> Result<()> {
    let count = self.read_count(parent.byte_order, "geometry")?;
    for _ in 0..count {
      self.read_geometry(polygon_ring_order, sink, depth)?;
    }
    Ok(())
  }

  fn read_count(&mut self, byte_order: ByteOrder, item: &str) -> Result<usize> {
    usize::try_from(self.read_u32(byte_order)?)
      .map_err(|_| anyhow::anyhow!("WKB {item} count does not fit usize"))
  }

  fn read_coordinate(&mut self, header: ParsedHeader) -> Result<(f64, f64)> {
    let x = self.read_f64(header.byte_order)?;
    let y = self.read_f64(header.byte_order)?;
    if header.has_z {
      self.read_f64(header.byte_order)?;
    }
    if header.has_m {
      self.read_f64(header.byte_order)?;
    }
    Ok((x, y))
  }

  fn read_u8(&mut self) -> Result<u8> {
    let value = *self
      .bytes
      .get(self.offset)
      .ok_or_else(|| anyhow::anyhow!("unexpected end of WKB at byte {}", self.offset))?;
    self.offset += 1;
    Ok(value)
  }

  fn read_u32(&mut self, byte_order: ByteOrder) -> Result<u32> {
    let bytes = self.read_array::<4>()?;
    Ok(match byte_order {
      ByteOrder::BigEndian => u32::from_be_bytes(bytes),
      ByteOrder::LittleEndian => u32::from_le_bytes(bytes),
    })
  }

  fn read_f64(&mut self, byte_order: ByteOrder) -> Result<f64> {
    let bytes = self.read_array::<8>()?;
    Ok(match byte_order {
      ByteOrder::BigEndian => f64::from_be_bytes(bytes),
      ByteOrder::LittleEndian => f64::from_le_bytes(bytes),
    })
  }

  fn read_f64_at(&self, offset: usize, byte_order: ByteOrder) -> Result<f64> {
    let end = offset
      .checked_add(size_of::<f64>())
      .ok_or_else(|| anyhow::anyhow!("WKB coordinate offset overflows usize"))?;
    let bytes: [u8; 8] = self
      .bytes
      .get(offset..end)
      .ok_or_else(|| anyhow::anyhow!("unexpected end of WKB at byte {offset}"))?
      .try_into()
      .expect("slice length matches array length");
    Ok(match byte_order {
      ByteOrder::BigEndian => f64::from_be_bytes(bytes),
      ByteOrder::LittleEndian => f64::from_le_bytes(bytes),
    })
  }

  fn read_array<const SIZE: usize>(&mut self) -> Result<[u8; SIZE]> {
    let end = self
      .offset
      .checked_add(SIZE)
      .ok_or_else(|| anyhow::anyhow!("WKB offset overflows usize"))?;
    let bytes = self
      .bytes
      .get(self.offset..end)
      .ok_or_else(|| anyhow::anyhow!("unexpected end of WKB at byte {}", self.offset))?;
    self.offset = end;
    Ok(bytes.try_into().expect("slice length matches array length"))
  }

  fn require_end(&self, end: usize) -> Result<()> {
    if end > self.bytes.len() {
      bail!(
        "unexpected end of WKB: geometry requires {end} bytes but buffer has {}",
        self.bytes.len()
      );
    }
    Ok(())
  }
}

fn decode_type(encoded_type: u32) -> Result<(u32, bool, bool, bool)> {
  if encoded_type & EWKB_BBOX != 0 {
    bail!("EWKB bounding-box headers are not supported");
  }
  if encoded_type & (EWKB_Z | EWKB_M | EWKB_SRID) != 0 {
    return Ok((
      encoded_type & EWKB_TYPE_MASK,
      encoded_type & EWKB_Z != 0,
      encoded_type & EWKB_M != 0,
      encoded_type & EWKB_SRID != 0,
    ));
  }
  let dimension_code = encoded_type / 1000;
  let base_type = encoded_type % 1000;
  let (has_z, has_m) = match dimension_code {
    0 => (false, false),
    1 => (true, false),
    2 => (false, true),
    3 => (true, true),
    _ => bail!("invalid ISO WKB dimensional type {encoded_type}"),
  };
  Ok((base_type, has_z, has_m, false))
}

fn decode_geometry_kind(base_type: u32) -> GeometryKind {
  match base_type {
    1 => GeometryKind::Point,
    2 => GeometryKind::LineString,
    3 => GeometryKind::Polygon,
    4 => GeometryKind::MultiPoint,
    5 => GeometryKind::MultiLineString,
    6 => GeometryKind::MultiPolygon,
    7 => GeometryKind::GeometryCollection,
    _ => GeometryKind::Unknown,
  }
}

fn dimensions(has_z: bool, has_m: bool) -> Dimensions {
  match (has_z, has_m) {
    (false, false) => Dimensions::Xy,
    (true, false) => Dimensions::Xyz,
    (false, true) => Dimensions::Xym,
    (true, true) => Dimensions::Xyzm,
  }
}

fn require_kind(actual: GeometryKind, expected: GeometryKind, context: &str) -> Result<()> {
  if actual != expected {
    bail!("{context} must be {expected:?}, found {actual:?}");
  }
  Ok(())
}

#[cfg(test)]
pub(crate) fn write_test_geometry(geometry: &geo::Geometry) -> Vec<u8> {
  let mut output = Vec::new();
  write_test_geometry_into(geometry, &mut output);
  output
}

#[cfg(test)]
fn write_test_geometry_into(geometry: &geo::Geometry, output: &mut Vec<u8>) {
  use geo::Geometry;
  match geometry {
    Geometry::Point(point) => {
      write_header(output, 1);
      write_coordinate(output, point.x(), point.y());
    }
    Geometry::LineString(line) => {
      write_header(output, 2);
      write_line_string(output, line);
    }
    Geometry::Polygon(polygon) => {
      write_header(output, 3);
      write_polygon(output, polygon);
    }
    Geometry::MultiPoint(points) => {
      write_header(output, 4);
      write_u32(output, points.0.len());
      for point in &points.0 {
        write_test_geometry_into(&Geometry::Point(*point), output);
      }
    }
    Geometry::MultiLineString(lines) => {
      write_header(output, 5);
      write_u32(output, lines.0.len());
      for line in &lines.0 {
        write_test_geometry_into(&Geometry::LineString(line.clone()), output);
      }
    }
    Geometry::MultiPolygon(polygons) => {
      write_header(output, 6);
      write_u32(output, polygons.0.len());
      for polygon in &polygons.0 {
        write_test_geometry_into(&Geometry::Polygon(polygon.clone()), output);
      }
    }
    Geometry::GeometryCollection(collection) => {
      write_header(output, 7);
      write_u32(output, collection.0.len());
      for member in &collection.0 {
        write_test_geometry_into(member, output);
      }
    }
    other => panic!("unsupported test geometry {other:?}"),
  }
}

#[cfg(test)]
fn write_polygon(output: &mut Vec<u8>, polygon: &geo::Polygon) {
  write_u32(output, 1 + polygon.interiors().len());
  write_line_string(output, polygon.exterior());
  for interior in polygon.interiors() {
    write_line_string(output, interior);
  }
}

#[cfg(test)]
fn write_line_string(output: &mut Vec<u8>, line: &geo::LineString) {
  write_u32(output, line.0.len());
  for coordinate in &line.0 {
    write_coordinate(output, coordinate.x, coordinate.y);
  }
}

#[cfg(test)]
fn write_header(output: &mut Vec<u8>, geometry_type: u32) {
  output.push(1);
  output.extend_from_slice(&geometry_type.to_le_bytes());
}

#[cfg(test)]
fn write_u32(output: &mut Vec<u8>, value: usize) {
  output.extend_from_slice(
    &u32::try_from(value)
      .expect("test geometry count fits u32")
      .to_le_bytes(),
  );
}

#[cfg(test)]
fn write_coordinate(output: &mut Vec<u8>, x: f64, y: f64) {
  output.extend_from_slice(&x.to_le_bytes());
  output.extend_from_slice(&y.to_le_bytes());
}

#[cfg(test)]
mod tests {
  use geo::polygon;

  use super::*;

  #[derive(Default)]
  struct CoordinateSink {
    roles: Vec<WkbPartRole>,
    coordinates: Vec<(f64, f64)>,
    lengths: Vec<usize>,
    current_length: usize,
  }

  impl WkbSink for CoordinateSink {
    fn start_part(&mut self, role: WkbPartRole) {
      self.roles.push(role);
      self.current_length = 0;
    }

    fn push_coord(&mut self, x: f64, y: f64) {
      self.coordinates.push((x, y));
      self.current_length += 1;
    }

    fn finish_part(&mut self) {
      self.lengths.push(self.current_length);
    }
  }

  #[test]
  fn reads_iso_dimensions() {
    for (encoded_type, dimensions, extra_values) in [
      (1_u32, Dimensions::Xy, vec![]),
      (1001_u32, Dimensions::Xyz, vec![3.0_f64]),
      (2001_u32, Dimensions::Xym, vec![4.0_f64]),
      (3001_u32, Dimensions::Xyzm, vec![3.0_f64, 4.0_f64]),
    ] {
      let mut bytes = vec![1];
      bytes.extend_from_slice(&encoded_type.to_le_bytes());
      bytes.extend_from_slice(&1.0_f64.to_le_bytes());
      bytes.extend_from_slice(&2.0_f64.to_le_bytes());
      for value in extra_values {
        bytes.extend_from_slice(&value.to_le_bytes());
      }
      let mut sink = CoordinateSink::default();
      let header = visit_wkb_geometry(&bytes, PolygonRingOrder::Preserve, &mut sink).unwrap();
      assert_eq!(header.dimensions, dimensions);
      assert_eq!(sink.coordinates, [(1.0, 2.0)]);
    }
  }

  #[test]
  fn reads_big_endian_point() {
    let mut bytes = vec![0];
    bytes.extend_from_slice(&1_u32.to_be_bytes());
    bytes.extend_from_slice(&1.0_f64.to_be_bytes());
    bytes.extend_from_slice(&2.0_f64.to_be_bytes());

    assert_eq!(read_wkb_point(&bytes).unwrap(), (1.0, 2.0));
  }

  #[test]
  fn reverses_polygon_rings_for_esri_order() {
    let polygon = polygon!(
      exterior: [
        (x: 0.0, y: 0.0),
        (x: 4.0, y: 0.0),
        (x: 4.0, y: 4.0),
        (x: 0.0, y: 4.0),
        (x: 0.0, y: 0.0),
      ],
      interiors: [[
        (x: 1.0, y: 1.0),
        (x: 1.0, y: 3.0),
        (x: 3.0, y: 3.0),
        (x: 3.0, y: 1.0),
        (x: 1.0, y: 1.0),
      ]],
    );
    let bytes = write_test_geometry(&geo::Geometry::Polygon(polygon));
    let mut sink = CoordinateSink::default();

    visit_wkb_geometry(&bytes, PolygonRingOrder::Reverse, &mut sink).unwrap();

    assert_eq!(
      sink.coordinates,
      [
        (0.0, 0.0),
        (0.0, 4.0),
        (4.0, 4.0),
        (4.0, 0.0),
        (0.0, 0.0),
        (1.0, 1.0),
        (3.0, 1.0),
        (3.0, 3.0),
        (1.0, 3.0),
        (1.0, 1.0),
      ]
    );
    assert_eq!(sink.roles, [WkbPartRole::Exterior, WkbPartRole::Interior]);
  }

  #[test]
  fn rejects_truncated_geometry_without_panicking() {
    let error = read_wkb_point(&[1, 1, 0, 0, 0]).unwrap_err();
    assert!(error.to_string().contains("unexpected end of WKB"));
  }

  #[test]
  fn validates_nested_member_types() {
    let mut bytes = vec![1];
    bytes.extend_from_slice(&4_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&write_test_geometry(&geo::Geometry::LineString(
      geo::LineString::from(vec![(0.0, 0.0), (1.0, 1.0)]),
    )));
    let mut sink = CoordinateSink::default();

    let error = visit_wkb_geometry(&bytes, PolygonRingOrder::Preserve, &mut sink).unwrap_err();

    assert!(
      error
        .to_string()
        .contains("MultiPoint member must be Point")
    );
  }

  #[test]
  fn reads_line_strings_in_all_iso_dimensions() {
    for dimension_code in [0_u32, 1, 2, 3] {
      let bytes = line_string_fixture(dimension_code, 3);
      let sink = decode_fixture(&bytes);
      assert_eq!(sink.coordinates, [(1.0, 2.0); 3]);
      assert_eq!(sink.lengths, [3]);
    }
  }

  #[test]
  fn reads_polygons_in_all_iso_dimensions() {
    for dimension_code in [0_u32, 1, 2, 3] {
      let bytes = polygon_fixture(dimension_code, &[3]);
      let sink = decode_fixture(&bytes);
      assert_eq!(sink.coordinates, [(1.0, 2.0); 3]);
      assert_eq!(sink.lengths, [3]);
      assert_eq!(sink.roles, [WkbPartRole::Exterior]);
    }
  }

  #[test]
  fn reads_multiple_polygon_rings_in_xyzm() {
    let bytes = polygon_fixture(3, &[3, 3]);
    let sink = decode_fixture(&bytes);

    assert_eq!(sink.coordinates, [(1.0, 2.0); 6]);
    assert_eq!(sink.lengths, [3, 3]);
    assert_eq!(sink.roles, [WkbPartRole::Exterior, WkbPartRole::Interior]);
  }

  #[test]
  fn reads_multi_points_in_all_iso_dimensions() {
    for dimension_code in [0_u32, 1, 2, 3] {
      let mut bytes = fixture_header(4, dimension_code);
      bytes.extend_from_slice(&2_u32.to_le_bytes());
      bytes.extend_from_slice(&point_fixture(dimension_code));
      bytes.extend_from_slice(&point_fixture(dimension_code));
      let sink = decode_fixture(&bytes);
      assert_eq!(sink.coordinates, [(1.0, 2.0); 2]);
      assert_eq!(sink.lengths, [2]);
    }
  }

  #[test]
  fn reads_multi_line_strings_in_all_iso_dimensions() {
    for dimension_code in [0_u32, 1, 2, 3] {
      let mut bytes = fixture_header(5, dimension_code);
      bytes.extend_from_slice(&2_u32.to_le_bytes());
      bytes.extend_from_slice(&line_string_fixture(dimension_code, 3));
      bytes.extend_from_slice(&line_string_fixture(dimension_code, 3));
      let sink = decode_fixture(&bytes);
      assert_eq!(sink.coordinates, [(1.0, 2.0); 6]);
      assert_eq!(sink.lengths, [3, 3]);
    }
  }

  #[test]
  fn reads_multi_polygons_in_all_iso_dimensions() {
    for dimension_code in [0_u32, 1, 2, 3] {
      let mut bytes = fixture_header(6, dimension_code);
      bytes.extend_from_slice(&2_u32.to_le_bytes());
      bytes.extend_from_slice(&polygon_fixture(dimension_code, &[3]));
      bytes.extend_from_slice(&polygon_fixture(dimension_code, &[3]));
      let sink = decode_fixture(&bytes);
      assert_eq!(sink.coordinates, [(1.0, 2.0); 6]);
      assert_eq!(sink.lengths, [3, 3]);
    }
  }

  #[test]
  fn reads_multiple_multi_polygon_rings_in_xyzm() {
    let mut bytes = fixture_header(6, 3);
    bytes.extend_from_slice(&2_u32.to_le_bytes());
    bytes.extend_from_slice(&polygon_fixture(3, &[3, 3]));
    bytes.extend_from_slice(&polygon_fixture(3, &[3, 3]));
    let sink = decode_fixture(&bytes);

    assert_eq!(sink.coordinates, [(1.0, 2.0); 12]);
    assert_eq!(sink.lengths, [3, 3, 3, 3]);
  }

  #[test]
  fn reads_ewkb_dimensions_and_srid() {
    let mut bytes = vec![1];
    bytes.extend_from_slice(&(EWKB_Z | EWKB_M | EWKB_SRID | 1).to_le_bytes());
    bytes.extend_from_slice(&4326_u32.to_le_bytes());
    write_fixture_coordinate(&mut bytes, 3);
    let mut sink = CoordinateSink::default();

    let header = visit_wkb_geometry(&bytes, PolygonRingOrder::Preserve, &mut sink).unwrap();

    assert_eq!(header.dimensions, Dimensions::Xyzm);
    assert_eq!(sink.coordinates, [(1.0, 2.0)]);
  }

  fn decode_fixture(bytes: &[u8]) -> CoordinateSink {
    let mut sink = CoordinateSink::default();
    visit_wkb_geometry(bytes, PolygonRingOrder::Preserve, &mut sink).unwrap();
    sink
  }

  fn point_fixture(dimension_code: u32) -> Vec<u8> {
    let mut bytes = fixture_header(1, dimension_code);
    write_fixture_coordinate(&mut bytes, dimension_code);
    bytes
  }

  fn line_string_fixture(dimension_code: u32, point_count: u32) -> Vec<u8> {
    let mut bytes = fixture_header(2, dimension_code);
    bytes.extend_from_slice(&point_count.to_le_bytes());
    for _ in 0..point_count {
      write_fixture_coordinate(&mut bytes, dimension_code);
    }
    bytes
  }

  fn polygon_fixture(dimension_code: u32, ring_point_counts: &[u32]) -> Vec<u8> {
    let mut bytes = fixture_header(3, dimension_code);
    bytes.extend_from_slice(
      &u32::try_from(ring_point_counts.len())
        .expect("fixture ring count fits u32")
        .to_le_bytes(),
    );
    for &point_count in ring_point_counts {
      bytes.extend_from_slice(&point_count.to_le_bytes());
      for _ in 0..point_count {
        write_fixture_coordinate(&mut bytes, dimension_code);
      }
    }
    bytes
  }

  fn fixture_header(base_type: u32, dimension_code: u32) -> Vec<u8> {
    let mut bytes = vec![1];
    bytes.extend_from_slice(&(dimension_code * 1000 + base_type).to_le_bytes());
    bytes
  }

  fn write_fixture_coordinate(bytes: &mut Vec<u8>, dimension_code: u32) {
    bytes.extend_from_slice(&1.0_f64.to_le_bytes());
    bytes.extend_from_slice(&2.0_f64.to_le_bytes());
    if matches!(dimension_code, 1 | 3) {
      bytes.extend_from_slice(&3.0_f64.to_le_bytes());
    }
    if matches!(dimension_code, 2 | 3) {
      bytes.extend_from_slice(&4.0_f64.to_le_bytes());
    }
  }
}

//! Reads and transforms ISO Well-Known Binary geometry.

use anyhow::{Result, bail};
use geo_traits::Dimensions;

use super::{Coord, Geometry, GeometryKind, GeometryType};

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

#[derive(Default)]
struct GeometryCollector {
  coordinates: Vec<Coord>,
  lengths: Vec<u32>,
  part_start: usize,
}

impl WkbSink for GeometryCollector {
  fn start_part(&mut self, _: WkbPartRole) {
    self.part_start = self.coordinates.len();
  }

  fn push_coord(&mut self, coordinate: WkbCoordinate) {
    self.coordinates.push(Coord {
      x: coordinate.x,
      y: coordinate.y,
      z: coordinate.z,
      m: coordinate.m,
    });
  }

  fn finish_part(&mut self) {
    self
      .lengths
      .push((self.coordinates.len() - self.part_start) as u32);
  }
}

pub(crate) type WkbCoordinate = Coord;

#[cfg(test)]
impl PartialEq<(f64, f64)> for WkbCoordinate {
  fn eq(&self, other: &(f64, f64)) -> bool {
    self.x == other.0 && self.y == other.1
  }
}

pub(crate) trait WkbSink {
  fn start_part(&mut self, role: WkbPartRole);
  fn push_coord(&mut self, coordinate: WkbCoordinate);
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

impl WkbHeader {
  /// Read the WKB header preceding one geometry payload.
  pub(crate) fn read(bytes: &[u8]) -> Result<Self> {
    let mut reader = WkbReader::new(bytes);
    Ok(reader.read_header()?.public)
  }
}

impl Geometry {
  /// Read supported WKB into the format-neutral geometry representation.
  pub(crate) fn from_wkb(bytes: &[u8]) -> Result<Self> {
    let mut collector = GeometryCollector::default();
    let header = visit_wkb_geometry(bytes, PolygonRingOrder::Preserve, &mut collector)?;
    Self::new(
      GeometryType::from_kind(header.kind)?,
      collector.coordinates,
      collector.lengths,
    )
  }
}

impl WkbCoordinate {
  pub(crate) fn from_point_wkb(bytes: &[u8]) -> Result<Self> {
    let mut reader = WkbReader::new(bytes);
    let header = reader.read_header()?;
    if header.public.kind != GeometryKind::Point {
      bail!("expected point WKB, found {:?}", header.public.kind);
    }
    reader.read_coordinate(header)
  }
}

pub(crate) fn strip_wkb_dimensions(bytes: &[u8], strip_z: bool, strip_m: bool) -> Result<Vec<u8>> {
  if !strip_z && !strip_m {
    return Ok(bytes.to_vec());
  }
  let mut reader = WkbReader::new(bytes);
  let mut output = Vec::with_capacity(bytes.len());
  reader.write_stripped_geometry(&mut output, strip_z, strip_m, 0)?;
  if reader.remaining() != 0 {
    bail!("WKB contains {} trailing bytes", reader.remaining());
  }
  Ok(output)
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

  fn write_stripped_geometry(
    &mut self,
    output: &mut Vec<u8>,
    strip_z: bool,
    strip_m: bool,
    depth: usize,
  ) -> Result<()> {
    if depth > MAX_NESTING_DEPTH {
      bail!("WKB nesting exceeds {MAX_NESTING_DEPTH} levels");
    }
    let header = self.read_header()?;
    let output_has_z = header.has_z && !strip_z;
    let output_has_m = header.has_m && !strip_m;
    write_iso_header(output, header.public.kind, output_has_z, output_has_m)?;
    match header.public.kind {
      GeometryKind::Point => {
        let coordinate = self.read_coordinate(header)?;
        write_selected_coordinate(output, coordinate, output_has_z, output_has_m);
      }
      GeometryKind::LineString => {
        self.write_stripped_coordinate_sequence(output, header, output_has_z, output_has_m)?;
      }
      GeometryKind::Polygon => {
        let ring_count = self.read_count(header.byte_order, "ring")?;
        write_count(output, ring_count)?;
        for _ in 0..ring_count {
          self.write_stripped_coordinate_sequence(output, header, output_has_z, output_has_m)?;
        }
      }
      GeometryKind::MultiPoint
      | GeometryKind::MultiLineString
      | GeometryKind::MultiPolygon
      | GeometryKind::GeometryCollection => {
        let count = self.read_count(header.byte_order, "geometry")?;
        write_count(output, count)?;
        for _ in 0..count {
          self.write_stripped_geometry(output, strip_z, strip_m, depth + 1)?;
        }
      }
      GeometryKind::Unknown => bail!("unsupported WKB geometry type"),
    }
    Ok(())
  }

  fn write_stripped_coordinate_sequence(
    &mut self,
    output: &mut Vec<u8>,
    header: ParsedHeader,
    output_has_z: bool,
    output_has_m: bool,
  ) -> Result<()> {
    let point_count = self.read_count(header.byte_order, "point")?;
    write_count(output, point_count)?;
    for _ in 0..point_count {
      let coordinate = self.read_coordinate(header)?;
      write_selected_coordinate(output, coordinate, output_has_z, output_has_m);
    }
    Ok(())
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
        kind: GeometryKind::from_wkb_type(base_type),
        dimensions: dimensions(has_z, has_m),
      },
      byte_order,
      has_z,
      has_m,
    })
  }

  fn read_point_part(&mut self, header: ParsedHeader, sink: &mut impl WkbSink) -> Result<()> {
    sink.start_part(WkbPartRole::Other);
    sink.push_coord(self.read_coordinate(header)?);
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
      sink.push_coord(self.read_coordinate(header)?);
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
      sink.push_coord(self.read_coordinate_at(point_offset, header)?);
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
      sink.push_coord(self.read_coordinate(header)?);
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

  fn read_coordinate(&mut self, header: ParsedHeader) -> Result<WkbCoordinate> {
    let x = self.read_f64(header.byte_order)?;
    let y = self.read_f64(header.byte_order)?;
    let z = header
      .has_z
      .then(|| self.read_f64(header.byte_order))
      .transpose()?;
    let m = header
      .has_m
      .then(|| self.read_f64(header.byte_order))
      .transpose()?;
    Ok(WkbCoordinate { x, y, z, m })
  }

  fn read_coordinate_at(&self, offset: usize, header: ParsedHeader) -> Result<WkbCoordinate> {
    let x = self.read_f64_at(offset, header.byte_order)?;
    let y = self.read_f64_at(offset + size_of::<f64>(), header.byte_order)?;
    let z = header
      .has_z
      .then(|| self.read_f64_at(offset + 2 * size_of::<f64>(), header.byte_order))
      .transpose()?;
    let m_offset = offset + (2 + usize::from(header.has_z)) * size_of::<f64>();
    let m = header
      .has_m
      .then(|| self.read_f64_at(m_offset, header.byte_order))
      .transpose()?;
    Ok(WkbCoordinate { x, y, z, m })
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

impl GeometryKind {
  fn from_wkb_type(base_type: u32) -> Self {
    match base_type {
      1 => Self::Point,
      2 => Self::LineString,
      3 => Self::Polygon,
      4 => Self::MultiPoint,
      5 => Self::MultiLineString,
      6 => Self::MultiPolygon,
      7 => Self::GeometryCollection,
      _ => Self::Unknown,
    }
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

fn write_iso_header(
  output: &mut Vec<u8>,
  kind: GeometryKind,
  has_z: bool,
  has_m: bool,
) -> Result<()> {
  let base_type: u32 = match kind {
    GeometryKind::Point => 1,
    GeometryKind::LineString => 2,
    GeometryKind::Polygon => 3,
    GeometryKind::MultiPoint => 4,
    GeometryKind::MultiLineString => 5,
    GeometryKind::MultiPolygon => 6,
    GeometryKind::GeometryCollection => 7,
    GeometryKind::Unknown => bail!("unsupported WKB geometry type"),
  };
  let dimension_offset: u32 = match (has_z, has_m) {
    (false, false) => 0,
    (true, false) => 1000,
    (false, true) => 2000,
    (true, true) => 3000,
  };
  output.push(1);
  output.extend_from_slice(&(base_type + dimension_offset).to_le_bytes());
  Ok(())
}

fn write_stripped_header(
  output: &mut Vec<u8>,
  base_type: u32,
  has_z: bool,
  has_m: bool,
) -> Result<()> {
  let dimension_offset = match (has_z, has_m) {
    (false, false) => 0,
    (true, false) => 1_000,
    (false, true) => 2_000,
    (true, true) => 3_000,
  };
  output.push(1);
  output.extend_from_slice(&(base_type + dimension_offset).to_le_bytes());
  Ok(())
}

fn write_count(output: &mut Vec<u8>, count: usize) -> Result<()> {
  output.extend_from_slice(
    &u32::try_from(count)
      .map_err(|_| anyhow::anyhow!("WKB count exceeds u32"))?
      .to_le_bytes(),
  );
  Ok(())
}

fn write_selected_coordinate(
  output: &mut Vec<u8>,
  coordinate: WkbCoordinate,
  has_z: bool,
  has_m: bool,
) {
  output.extend_from_slice(&coordinate.x.to_le_bytes());
  output.extend_from_slice(&coordinate.y.to_le_bytes());
  if has_z {
    output.extend_from_slice(
      &coordinate
        .z
        .expect("selected Z originates from dimensional WKB")
        .to_le_bytes(),
    );
  }
  if has_m {
    output.extend_from_slice(
      &coordinate
        .m
        .expect("selected M originates from dimensional WKB")
        .to_le_bytes(),
    );
  }
}

fn require_kind(actual: GeometryKind, expected: GeometryKind, context: &str) -> Result<()> {
  if actual != expected {
    bail!("{context} must be {expected:?}, found {actual:?}");
  }
  Ok(())
}

/// Write normalized geometry as deterministic little-endian WKB.
///
/// Polylines with one part encode as `LineString` and multiple parts encode as
/// `MultiLineString`. Polygon parts encode as rings. The normalized model deliberately maps
/// multipolygons to polygon rings because optimized output does not retain polygon grouping.
impl Geometry {
  pub(crate) fn to_wkb(&self) -> Result<Vec<u8>> {
    let has_z = self
      .coordinates
      .iter()
      .all(|coordinate| coordinate.z.is_some());
    let has_m = self
      .coordinates
      .iter()
      .all(|coordinate| coordinate.m.is_some());
    if self
      .coordinates
      .iter()
      .any(|coordinate| coordinate.z.is_some() != has_z)
      || self
        .coordinates
        .iter()
        .any(|coordinate| coordinate.m.is_some() != has_m)
    {
      bail!("WKB output requires consistent Z and M dimensions");
    }

    let mut output = Vec::new();
    match self.ty {
      GeometryType::Point => {
        if self.lengths != [1] {
          bail!("point geometry must contain exactly one coordinate");
        }
        write_stripped_header(&mut output, 1, has_z, has_m)?;
        Self::write_wkb_coordinate(&mut output, self.coordinates[0], has_z, has_m);
      }
      GeometryType::MultiPoint => {
        write_stripped_header(&mut output, 4, has_z, has_m)?;
        write_count(&mut output, self.coordinates.len())?;
        for coordinate in &self.coordinates {
          write_stripped_header(&mut output, 1, has_z, has_m)?;
          Self::write_wkb_coordinate(&mut output, *coordinate, has_z, has_m);
        }
      }
      GeometryType::Polyline => {
        if self.lengths.len() == 1 {
          write_stripped_header(&mut output, 2, has_z, has_m)?;
          self.write_wkb_part(&mut output, 0, has_z, has_m)?;
        } else {
          write_stripped_header(&mut output, 5, has_z, has_m)?;
          write_count(&mut output, self.lengths.len())?;
          for part_index in 0..self.lengths.len() {
            write_stripped_header(&mut output, 2, has_z, has_m)?;
            self.write_wkb_part(&mut output, part_index, has_z, has_m)?;
          }
        }
      }
      GeometryType::Polygon => {
        write_stripped_header(&mut output, 3, has_z, has_m)?;
        write_count(&mut output, self.lengths.len())?;
        for part_index in 0..self.lengths.len() {
          self.write_wkb_part(&mut output, part_index, has_z, has_m)?;
        }
      }
    }
    Ok(output)
  }

  fn write_wkb_part(
    &self,
    output: &mut Vec<u8>,
    part_index: usize,
    has_z: bool,
    has_m: bool,
  ) -> Result<()> {
    let start = self.lengths[..part_index]
      .iter()
      .map(|length| *length as usize)
      .sum::<usize>();
    let length = self.lengths[part_index] as usize;
    write_count(output, length)?;
    for coordinate in &self.coordinates[start..start + length] {
      Self::write_wkb_coordinate(output, *coordinate, has_z, has_m);
    }
    Ok(())
  }

  fn write_wkb_coordinate(output: &mut Vec<u8>, coordinate: Coord, has_z: bool, has_m: bool) {
    output.extend_from_slice(&coordinate.x.to_le_bytes());
    output.extend_from_slice(&coordinate.y.to_le_bytes());
    if has_z {
      output.extend_from_slice(&coordinate.z.expect("validated Z dimension").to_le_bytes());
    }
    if has_m {
      output.extend_from_slice(&coordinate.m.expect("validated M dimension").to_le_bytes());
    }
  }
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
    coordinates: Vec<WkbCoordinate>,
    lengths: Vec<usize>,
    current_length: usize,
  }

  impl WkbSink for CoordinateSink {
    fn start_part(&mut self, role: WkbPartRole) {
      self.roles.push(role);
      self.current_length = 0;
    }

    fn push_coord(&mut self, coordinate: WkbCoordinate) {
      self.coordinates.push(coordinate);
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
      assert_eq!(
        sink.coordinates,
        [WkbCoordinate {
          x: 1.0,
          y: 2.0,
          z: matches!(dimensions, Dimensions::Xyz | Dimensions::Xyzm).then_some(3.0),
          m: matches!(dimensions, Dimensions::Xym | Dimensions::Xyzm).then_some(4.0),
        }]
      );
    }
  }

  #[test]
  fn reads_big_endian_point() {
    let mut bytes = vec![0];
    bytes.extend_from_slice(&1_u32.to_be_bytes());
    bytes.extend_from_slice(&1.0_f64.to_be_bytes());
    bytes.extend_from_slice(&2.0_f64.to_be_bytes());

    assert_eq!(
      WkbCoordinate::from_point_wkb(&bytes).unwrap(),
      WkbCoordinate {
        x: 1.0,
        y: 2.0,
        z: None,
        m: None,
      }
    );
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
  fn reversing_polygon_rings_preserves_z_and_m_with_each_vertex() {
    let mut bytes = vec![1];
    bytes.extend_from_slice(&3003_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&4_u32.to_le_bytes());
    for coordinate in [
      WkbCoordinate {
        x: 0.0,
        y: 0.0,
        z: Some(10.0),
        m: Some(100.0),
      },
      WkbCoordinate {
        x: 2.0,
        y: 0.0,
        z: Some(20.0),
        m: Some(200.0),
      },
      WkbCoordinate {
        x: 0.0,
        y: 2.0,
        z: Some(30.0),
        m: Some(300.0),
      },
      WkbCoordinate {
        x: 0.0,
        y: 0.0,
        z: Some(10.0),
        m: Some(100.0),
      },
    ] {
      bytes.extend_from_slice(&coordinate.x.to_le_bytes());
      bytes.extend_from_slice(&coordinate.y.to_le_bytes());
      bytes.extend_from_slice(&coordinate.z.unwrap().to_le_bytes());
      bytes.extend_from_slice(&coordinate.m.unwrap().to_le_bytes());
    }
    let mut sink = CoordinateSink::default();

    visit_wkb_geometry(&bytes, PolygonRingOrder::Reverse, &mut sink).unwrap();

    assert_eq!(
      sink.coordinates,
      [
        WkbCoordinate {
          x: 0.0,
          y: 0.0,
          z: Some(10.0),
          m: Some(100.0),
        },
        WkbCoordinate {
          x: 0.0,
          y: 2.0,
          z: Some(30.0),
          m: Some(300.0),
        },
        WkbCoordinate {
          x: 2.0,
          y: 0.0,
          z: Some(20.0),
          m: Some(200.0),
        },
        WkbCoordinate {
          x: 0.0,
          y: 0.0,
          z: Some(10.0),
          m: Some(100.0),
        },
      ]
    );
  }

  #[test]
  fn reads_wkb_into_format_neutral_geometry() {
    let bytes = write_test_geometry(&geo::Geometry::LineString(geo::LineString::from(vec![
      (0.0, 1.0),
      (2.0, 3.0),
    ])));

    let geometry = Geometry::from_wkb(&bytes).unwrap();

    assert_eq!(geometry.ty, GeometryType::Polyline);
    assert_eq!(geometry.lengths, [2]);
    assert_eq!(geometry.coordinates[1].x, 2.0);
    assert_eq!(geometry.coordinates[1].y, 3.0);
  }

  #[test]
  fn writes_format_neutral_geometry_as_wkb() {
    let geometry = Geometry::new(
      GeometryType::Polyline,
      vec![
        Coord {
          x: 0.0,
          y: 1.0,
          z: None,
          m: None,
        },
        Coord {
          x: 2.0,
          y: 3.0,
          z: None,
          m: None,
        },
      ],
      vec![2],
    )
    .unwrap();

    let bytes = geometry.to_wkb().unwrap();

    assert_eq!(Geometry::from_wkb(&bytes).unwrap(), geometry);
  }

  #[test]
  fn rejects_truncated_geometry_without_panicking() {
    let error = WkbCoordinate::from_point_wkb(&[1, 1, 0, 0, 0]).unwrap_err();
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

  #[test]
  fn strips_z_and_m_independently_from_xyzm_wkb() {
    let input = point_fixture(3);
    for (strip_z, strip_m, expected_dimensions, expected_z, expected_m) in [
      (false, false, Dimensions::Xyzm, Some(3.0), Some(4.0)),
      (true, false, Dimensions::Xym, None, Some(4.0)),
      (false, true, Dimensions::Xyz, Some(3.0), None),
      (true, true, Dimensions::Xy, None, None),
    ] {
      let output = strip_wkb_dimensions(&input, strip_z, strip_m).unwrap();
      let header = WkbHeader::read(&output).unwrap();
      let coordinate = WkbCoordinate::from_point_wkb(&output).unwrap();

      assert_eq!(header.dimensions, expected_dimensions);
      assert_eq!(coordinate.z, expected_z);
      assert_eq!(coordinate.m, expected_m);
    }
  }

  #[test]
  fn strips_dimensions_through_nested_wkb_members() {
    let mut input = fixture_header(6, 3);
    input.extend_from_slice(&1_u32.to_le_bytes());
    input.extend_from_slice(&polygon_fixture(3, &[4]));

    let output = strip_wkb_dimensions(&input, true, false).unwrap();
    let mut sink = CoordinateSink::default();
    let header = visit_wkb_geometry(&output, PolygonRingOrder::Preserve, &mut sink).unwrap();

    assert_eq!(header.dimensions, Dimensions::Xym);
    assert_eq!(sink.lengths, [4]);
    assert!(
      sink
        .coordinates
        .iter()
        .all(|coordinate| { coordinate.z.is_none() && coordinate.m == Some(4.0) })
    );
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

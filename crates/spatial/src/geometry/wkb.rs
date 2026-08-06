//! Reads and transforms ISO Well-Known Binary geometry.

use super::{
  Coord, CoordinateDimensions, CoordinateSpace, Geometry, GeometryError, GeometryKind,
  GeometryType, QuantizationTransform, QuantizedGeometry,
};

use arrow_array::ArrayRef;
use arrow_array::builder::BinaryBuilder;
use std::sync::Arc;

type Result<T> = std::result::Result<T, GeometryError>;

macro_rules! bail {
  ($($argument:tt)*) => {
    return Err(GeometryError::Wkb(format!($($argument)*)))
  };
}

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
  kind: Option<GeometryKind>,
  coordinates: Vec<Coord>,
  lengths: Vec<u32>,
  part_start: usize,
  active_polygon_ring_count: Option<u32>,
  polygon_ring_counts: Vec<u32>,
}

impl WkbSink for GeometryCollector {
  fn start_geometry(&mut self, kind: GeometryKind) {
    if self.kind.is_none() {
      self.kind = Some(kind);
    }
    if kind == GeometryKind::Polygon {
      self.active_polygon_ring_count = Some(0);
    }
  }

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
    if let Some(ring_count) = self.active_polygon_ring_count.as_mut() {
      *ring_count += 1;
    }
  }

  fn finish_geometry(&mut self, kind: GeometryKind) {
    if kind == GeometryKind::Polygon {
      self.polygon_ring_counts.push(
        self
          .active_polygon_ring_count
          .take()
          .expect("started polygon geometry"),
      );
    }
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
  fn start_geometry(&mut self, _: GeometryKind) {}
  fn start_part(&mut self, role: WkbPartRole);
  fn push_coord(&mut self, coordinate: WkbCoordinate);
  fn finish_part(&mut self);
  fn finish_geometry(&mut self, _: GeometryKind) {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PolygonRingOrder {
  Preserve,
  Reverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WkbHeader {
  pub(crate) kind: GeometryKind,
  pub(crate) dimensions: CoordinateDimensions,
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
    visit_wkb_geometry(bytes, PolygonRingOrder::Preserve, &mut collector)?;
    Self::new(
      collector
        .kind
        .expect("WKB visitor starts the root geometry"),
      collector.coordinates,
      collector.lengths,
      collector.polygon_ring_counts,
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

/// Builds a BinaryArray containing one transformed WKB value per multiscale geometry row.
pub(crate) struct WkbArrayBuilder {
  builder: BinaryBuilder,
  scratch: Vec<u8>,
  coordinate_space: CoordinateSpace,
}

impl WkbArrayBuilder {
  /// Create a WKB level builder with capacity for the expected row count.
  pub(crate) fn new(capacity: usize, coordinate_space: CoordinateSpace) -> Self {
    Self {
      builder: BinaryBuilder::with_capacity(capacity, capacity * 16),
      scratch: Vec::new(),
      coordinate_space,
    }
  }

  /// Append one simplified quantized geometry as transformed ISO WKB.
  pub(crate) fn append(
    &mut self,
    geometry: &QuantizedGeometry,
    transform: &QuantizationTransform,
  ) -> Result<()> {
    self.scratch.clear();
    write_quantized_geometry(
      &mut self.scratch,
      geometry,
      self.coordinate_space,
      transform,
    )?;
    self.builder.append_value(&self.scratch);
    Ok(())
  }

  /// Append a null geometry row.
  pub(crate) fn append_null(&mut self) {
    self.builder.append_null();
  }

  /// Finish the Arrow binary array.
  pub(crate) fn finish(mut self) -> ArrayRef {
    Arc::new(self.builder.finish())
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
    sink.start_geometry(header.public.kind);
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
    sink.finish_geometry(header.public.kind);
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
      .ok_or_else(|| GeometryError::Wkb("WKB coordinate width overflows usize".to_string()))?;
    let coordinate_bytes = point_count
      .checked_mul(coordinate_width)
      .ok_or_else(|| GeometryError::Wkb("WKB ring byte length overflows usize".to_string()))?;
    let ring_start = self.offset;
    let ring_end = ring_start
      .checked_add(coordinate_bytes)
      .ok_or_else(|| GeometryError::Wkb("WKB ring end offset overflows usize".to_string()))?;
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
      sink.start_geometry(GeometryKind::Polygon);
      self.read_polygon(header, polygon_ring_order, sink)?;
      sink.finish_geometry(GeometryKind::Polygon);
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
      .map_err(|_| GeometryError::Wkb(format!("WKB {item} count does not fit usize")))
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
    let value = *self.bytes.get(self.offset).ok_or_else(|| {
      GeometryError::Wkb(format!("unexpected end of WKB at byte {}", self.offset))
    })?;
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
      .ok_or_else(|| GeometryError::Wkb("WKB coordinate offset overflows usize".to_string()))?;
    let bytes: [u8; 8] = self
      .bytes
      .get(offset..end)
      .ok_or_else(|| GeometryError::Wkb(format!("unexpected end of WKB at byte {offset}")))?
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
      .ok_or_else(|| GeometryError::Wkb("WKB offset overflows usize".to_string()))?;
    let bytes = self.bytes.get(self.offset..end).ok_or_else(|| {
      GeometryError::Wkb(format!("unexpected end of WKB at byte {}", self.offset))
    })?;
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

impl GeometryType {
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

fn dimensions(has_z: bool, has_m: bool) -> CoordinateDimensions {
  match (has_z, has_m) {
    (false, false) => CoordinateDimensions::Xy,
    (true, false) => CoordinateDimensions::Xyz,
    (true, true) => CoordinateDimensions::Xyzm,
    (false, true) => CoordinateDimensions::Xym,
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

fn write_count(output: &mut Vec<u8>, count: usize) -> Result<()> {
  output.extend_from_slice(
    &u32::try_from(count)
      .map_err(|_| GeometryError::Wkb("WKB count exceeds u32".to_string()))?
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

fn write_quantized_geometry(
  output: &mut Vec<u8>,
  geometry: &QuantizedGeometry,
  coordinate_space: CoordinateSpace,
  transform: &QuantizationTransform,
) -> Result<()> {
  let mut part_index = 0usize;
  match geometry.kind {
    GeometryKind::LineString => {
      write_iso_header(
        output,
        GeometryKind::LineString,
        geometry.has_z,
        geometry.has_m,
      )?;
      write_quantized_part(
        output,
        geometry,
        coordinate_space,
        transform,
        &mut part_index,
      )?;
    }
    GeometryKind::MultiLineString => {
      write_iso_header(
        output,
        GeometryKind::MultiLineString,
        geometry.has_z,
        geometry.has_m,
      )?;
      write_count(output, geometry.lengths.len())?;
      for _ in &geometry.lengths {
        write_iso_header(
          output,
          GeometryKind::LineString,
          geometry.has_z,
          geometry.has_m,
        )?;
        write_quantized_part(
          output,
          geometry,
          coordinate_space,
          transform,
          &mut part_index,
        )?;
      }
    }
    GeometryKind::Polygon => {
      write_iso_header(
        output,
        GeometryKind::Polygon,
        geometry.has_z,
        geometry.has_m,
      )?;
      write_quantized_polygon(
        output,
        geometry,
        coordinate_space,
        transform,
        &mut part_index,
        geometry.polygon_ring_counts.first().copied().unwrap_or(0) as usize,
      )?;
    }
    GeometryKind::MultiPolygon => {
      write_iso_header(
        output,
        GeometryKind::MultiPolygon,
        geometry.has_z,
        geometry.has_m,
      )?;
      write_count(output, geometry.polygon_ring_counts.len())?;
      for &ring_count in &geometry.polygon_ring_counts {
        write_iso_header(
          output,
          GeometryKind::Polygon,
          geometry.has_z,
          geometry.has_m,
        )?;
        write_quantized_polygon(
          output,
          geometry,
          coordinate_space,
          transform,
          &mut part_index,
          ring_count as usize,
        )?;
      }
    }
    GeometryKind::MultiPoint => {
      write_iso_header(
        output,
        GeometryKind::MultiPoint,
        geometry.has_z,
        geometry.has_m,
      )?;
      let stride = coordinate_stride(geometry.has_z, geometry.has_m);
      write_count(output, geometry.coordinates.len() / stride)?;
      for coordinate_index in 0..geometry.coordinates.len() / stride {
        write_iso_header(output, GeometryKind::Point, geometry.has_z, geometry.has_m)?;
        write_quantized_coordinate(
          output,
          geometry,
          coordinate_space,
          transform,
          coordinate_index,
        )?;
      }
    }
    GeometryKind::Point => {
      write_iso_header(output, GeometryKind::Point, geometry.has_z, geometry.has_m)?;
      write_quantized_coordinate(output, geometry, coordinate_space, transform, 0)?;
    }
    GeometryKind::GeometryCollection | GeometryKind::Unknown => {
      bail!(
        "unsupported quantized WKB geometry type {:?}",
        geometry.kind
      )
    }
  }
  Ok(())
}

fn write_quantized_polygon(
  output: &mut Vec<u8>,
  geometry: &QuantizedGeometry,
  coordinate_space: CoordinateSpace,
  transform: &QuantizationTransform,
  part_index: &mut usize,
  ring_count: usize,
) -> Result<()> {
  write_count(output, ring_count)?;
  for _ in 0..ring_count {
    write_quantized_part(output, geometry, coordinate_space, transform, part_index)?;
  }
  Ok(())
}

fn write_quantized_part(
  output: &mut Vec<u8>,
  geometry: &QuantizedGeometry,
  coordinate_space: CoordinateSpace,
  transform: &QuantizationTransform,
  part_index: &mut usize,
) -> Result<()> {
  let point_count = *geometry.lengths.get(*part_index).ok_or_else(|| {
    GeometryError::Wkb("quantized geometry part count does not match topology".to_string())
  })? as usize;
  write_count(output, point_count)?;
  let coordinate_start = geometry.lengths[..*part_index]
    .iter()
    .map(|length| *length as usize)
    .sum();
  for coordinate_index in coordinate_start..coordinate_start + point_count {
    write_quantized_coordinate(
      output,
      geometry,
      coordinate_space,
      transform,
      coordinate_index,
    )?;
  }
  *part_index += 1;
  Ok(())
}

fn write_quantized_coordinate(
  output: &mut Vec<u8>,
  geometry: &QuantizedGeometry,
  coordinate_space: CoordinateSpace,
  transform: &QuantizationTransform,
  coordinate_index: usize,
) -> Result<()> {
  let stride = coordinate_stride(geometry.has_z, geometry.has_m);
  let offset = coordinate_index
    .checked_mul(stride)
    .ok_or_else(|| GeometryError::Wkb("quantized coordinate offset overflows usize".to_string()))?;
  let coordinate = geometry
    .coordinates
    .get(offset..offset + stride)
    .ok_or_else(|| {
      GeometryError::Wkb("quantized coordinate does not match part lengths".to_string())
    })?;
  output.extend_from_slice(
    &coordinate_space
      .decode(coordinate[0], transform, 0)
      .to_le_bytes(),
  );
  output.extend_from_slice(
    &coordinate_space
      .decode(coordinate[1], transform, 1)
      .to_le_bytes(),
  );
  let mut component_index = 2;
  if geometry.has_z {
    let value = if geometry.validity.z_is_valid(coordinate_index) {
      coordinate_space.decode(coordinate[component_index], transform, 2)
    } else {
      0.0
    };
    output.extend_from_slice(&value.to_le_bytes());
    component_index += 1;
  }
  if geometry.has_m {
    let value = if geometry.validity.m_is_valid(coordinate_index) {
      coordinate_space.decode(coordinate[component_index], transform, 3)
    } else {
      0.0
    };
    output.extend_from_slice(&value.to_le_bytes());
  }
  Ok(())
}

fn coordinate_stride(has_z: bool, has_m: bool) -> usize {
  2 + usize::from(has_z) + usize::from(has_m)
}

fn require_kind(actual: GeometryKind, expected: GeometryKind, context: &str) -> Result<()> {
  if actual != expected {
    bail!("{context} must be {expected:?}, found {actual:?}");
  }
  Ok(())
}

#[cfg(test)]
pub(crate) fn write_test_point(x: f64, y: f64) -> Vec<u8> {
  let mut output = Vec::new();
  write_header(&mut output, 1);
  write_coordinate(&mut output, x, y);
  output
}

#[cfg(test)]
pub(crate) fn write_test_line_string(coordinates: &[(f64, f64)]) -> Vec<u8> {
  let mut output = Vec::new();
  write_header(&mut output, 2);
  write_test_coordinate_sequence(&mut output, coordinates);
  output
}

#[cfg(test)]
pub(crate) fn write_test_polygon(rings: &[&[(f64, f64)]]) -> Vec<u8> {
  let mut output = Vec::new();
  write_header(&mut output, 3);
  write_u32(&mut output, rings.len());
  for ring in rings {
    write_test_coordinate_sequence(&mut output, ring);
  }
  output
}

#[cfg(test)]
fn write_test_coordinate_sequence(output: &mut Vec<u8>, coordinates: &[(f64, f64)]) {
  write_u32(output, coordinates.len());
  for &(x, y) in coordinates {
    write_coordinate(output, x, y);
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
  use super::*;
  use crate::geometry::GeometryType;

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
      (1_u32, CoordinateDimensions::Xy, vec![]),
      (1001_u32, CoordinateDimensions::Xyz, vec![3.0_f64]),
      (2001_u32, CoordinateDimensions::Xym, vec![4.0_f64]),
      (3001_u32, CoordinateDimensions::Xyzm, vec![3.0_f64, 4.0_f64]),
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
          z: matches!(
            dimensions,
            CoordinateDimensions::Xyz | CoordinateDimensions::Xyzm
          )
          .then_some(3.0),
          m: matches!(
            dimensions,
            CoordinateDimensions::Xym | CoordinateDimensions::Xyzm
          )
          .then_some(4.0),
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
    let exterior = [(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0), (0.0, 0.0)];
    let interior = [(1.0, 1.0), (1.0, 3.0), (3.0, 3.0), (3.0, 1.0), (1.0, 1.0)];
    let bytes = write_test_polygon(&[&exterior, &interior]);
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
    let bytes = write_test_line_string(&[(0.0, 1.0), (2.0, 3.0)]);

    let geometry = Geometry::from_wkb(&bytes).unwrap();

    assert_eq!(geometry.ty, GeometryType::LineString);
    assert_eq!(geometry.lengths, [2]);
    assert_eq!(geometry.coordinates[1].x, 2.0);
    assert_eq!(geometry.coordinates[1].y, 3.0);
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
    bytes.extend_from_slice(&write_test_line_string(&[(0.0, 0.0), (1.0, 1.0)]));
    let mut sink = CoordinateSink::default();

    let error = visit_wkb_geometry(&bytes, PolygonRingOrder::Preserve, &mut sink).unwrap_err();

    assert!(
      error
        .to_string()
        .contains("MultiPoint member must be Point")
    );
  }

  #[test]
  fn writes_transformed_multipolygon_wkb_with_ring_groups() {
    let geometry = QuantizedGeometry {
      kind: GeometryKind::MultiPolygon,
      coordinates: vec![0, 0, 2, 0, 2, 2, 0, 0, 4, 4, 6, 4, 6, 6, 4, 4],
      lengths: vec![4, 4],
      polygon_ring_counts: vec![1, 1],
      validity: Default::default(),
      has_z: false,
      has_m: false,
    };
    let transform = QuantizationTransform {
      scale: [0.5, 0.5, 1.0, 1.0],
      translate: [10.0, 20.0, 0.0, 0.0],
    };
    let mut bytes = Vec::new();

    write_quantized_geometry(&mut bytes, &geometry, CoordinateSpace::World, &transform).unwrap();
    let decoded = Geometry::from_wkb(&bytes).unwrap();

    assert_eq!(decoded.ty, GeometryKind::MultiPolygon);
    assert_eq!(decoded.polygon_ring_counts, [1, 1]);
    assert_eq!(
      decoded.coordinates[0],
      Coord {
        x: 10.0,
        y: 20.0,
        z: None,
        m: None,
      }
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

    assert_eq!(header.dimensions, CoordinateDimensions::Xyzm);
    assert_eq!(sink.coordinates, [(1.0, 2.0)]);
  }

  #[test]
  fn strips_z_and_m_independently_from_xyzm_wkb() {
    let input = point_fixture(3);
    for (strip_z, strip_m, expected_dimensions, expected_z, expected_m) in [
      (
        false,
        false,
        CoordinateDimensions::Xyzm,
        Some(3.0),
        Some(4.0),
      ),
      (true, false, CoordinateDimensions::Xym, None, Some(4.0)),
      (false, true, CoordinateDimensions::Xyz, Some(3.0), None),
      (true, true, CoordinateDimensions::Xy, None, None),
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

    assert_eq!(header.dimensions, CoordinateDimensions::Xym);
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

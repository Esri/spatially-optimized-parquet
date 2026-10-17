// Copyright 2026 Esri
//
// Licensed under the Apache License Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Reads ISO WKB and EWKB into shared coordinates and writes transformed ISO WKB.
//!
//! The reader visits parts without allocating coordinate buffers. The collector builds a
//! [`Geometry`] when processing needs owned coordinates. The writer restores world coordinates
//! from [`QuantizedGeometry`], preserving part boundaries and polygon ring groups.

use super::{
  Coord, CoordinateDimensions, Geometry, GeometryError, GeometryType, QuantizationTransform,
  QuantizedGeometry,
};

use arrow_array::ArrayRef;
use arrow_array::builder::BinaryBuilder;
use std::sync::Arc;

type Result<T> = std::result::Result<T, GeometryError>;

/// Defines ordered geometry, part, and vertex events for WKB consumers.
pub(crate) trait WkbSink {
  /// Start one geometry before visiting its parts or nested members.
  fn start_geometry(&mut self, _: GeometryType) {}
  /// Start a coordinate part with its polygon role when applicable.
  fn start_part(&mut self, role: WkbPartRole);
  /// Append one source vertex with its optional Z/M components.
  fn push_coord(&mut self, coordinate: Coord);
  /// Finish the current coordinate part.
  fn finish_part(&mut self);
  /// Finish one geometry after visiting its parts or nested members.
  fn finish_geometry(&mut self, _: GeometryType) {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WkbPartRole {
  Exterior,
  Interior,
  Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PolygonRingOrder {
  Preserve,
  Reverse,
}

#[derive(Debug, Clone, Copy)]
enum ByteOrder {
  BigEndian,
  LittleEndian,
}

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
pub(crate) struct WkbHeader {
  pub(crate) ty: GeometryType,
  pub(crate) dimensions: CoordinateDimensions,
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
      collector.ty.expect("WKB visitor starts the root geometry"),
      collector.coordinates,
      collector.lengths,
      collector.polygon_ring_counts,
    )
  }
}

impl Coord {
  /// Read one point coordinate from WKB with its optional Z/M components.
  pub(crate) fn from_point_wkb(bytes: &[u8]) -> Result<Self> {
    let mut reader = WkbReader::new(bytes);
    let header = reader.read_header()?;
    if header.public.ty != GeometryType::Point {
      bail!("expected point WKB, found {:?}", header.public.ty);
    }
    reader.read_coordinate(header)
  }
}

impl GeometryType {
  /// Resolve the concrete geometry type from a WKB base type code.
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

/// Owns reusable byte storage for writing transformed WKB rows into an Arrow binary array.
pub(crate) struct WkbArrayBuilder {
  builder: BinaryBuilder,
  buffer: Vec<u8>,
}

impl WkbArrayBuilder {
  /// Create a WKB level builder with capacity for the expected row count.
  pub(crate) fn new(capacity: usize) -> Self {
    Self {
      builder: BinaryBuilder::with_capacity(capacity, capacity * 16),
      buffer: Vec::new(),
    }
  }

  /// Append one simplified quantized geometry as transformed ISO WKB.
  pub(crate) fn append(
    &mut self,
    geometry: &QuantizedGeometry,
    transform: &QuantizationTransform,
  ) -> Result<()> {
    self.buffer.clear();
    QuantizedWkbWriter::new(&mut self.buffer, geometry, transform).write_geometry()?;
    self.builder.append_value(&self.buffer);
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

/// Write ISO WKB from source bytes with the requested Z/M components removed.
pub(crate) fn strip_wkb_dimensions(bytes: &[u8], strip_z: bool, strip_m: bool) -> Result<Vec<u8>> {
  if !strip_z && !strip_m {
    return Ok(bytes.to_vec());
  }
  let mut reader = WkbReader::new(bytes);
  let mut output = Vec::with_capacity(bytes.len());
  reader.write_stripped_geometry(&mut output, strip_z, strip_m, 0)?;
  reader.require_finished()?;
  Ok(output)
}

/// Visit source WKB parts in the requested ring order, rejecting incomplete or trailing bytes.
pub(crate) fn visit_wkb_geometry(
  bytes: &[u8],
  polygon_ring_order: PolygonRingOrder,
  sink: &mut impl WkbSink,
) -> Result<WkbHeader> {
  let mut reader = WkbReader::new(bytes);
  let header = reader.read_geometry(polygon_ring_order, sink, 0)?;
  reader.require_finished()?;
  Ok(header)
}

#[cfg(test)]
/// Build an XY point WKB fixture.
pub(crate) fn write_test_point(x: f64, y: f64) -> Vec<u8> {
  let mut output = Vec::new();
  write_header(&mut output, 1);
  write_coordinate(&mut output, x, y);
  output
}

#[cfg(test)]
/// Build an XY line string WKB fixture.
pub(crate) fn write_test_line_string(coordinates: &[(f64, f64)]) -> Vec<u8> {
  let mut output = Vec::new();
  write_header(&mut output, 2);
  write_test_coordinate_sequence(&mut output, coordinates);
  output
}

#[cfg(test)]
/// Build an XY polygon WKB fixture from ordered rings.
pub(crate) fn write_test_polygon(rings: &[&[(f64, f64)]]) -> Vec<u8> {
  let mut output = Vec::new();
  write_header(&mut output, 3);
  write_u32(&mut output, rings.len());
  for ring in rings {
    write_test_coordinate_sequence(&mut output, ring);
  }
  output
}

#[derive(Default)]
/// Owns decoded coordinates, part lengths, and polygon groups for a shared geometry.
struct GeometryCollector {
  ty: Option<GeometryType>,
  coordinates: Vec<Coord>,
  lengths: Vec<u32>,
  part_start: usize,
  active_polygon_ring_count: Option<u32>,
  polygon_ring_counts: Vec<u32>,
}

impl WkbSink for GeometryCollector {
  /// Record the root type and start polygon ring counting.
  fn start_geometry(&mut self, ty: GeometryType) {
    if self.ty.is_none() {
      self.ty = Some(ty);
    }
    if ty == GeometryType::Polygon {
      self.active_polygon_ring_count = Some(0);
    }
  }

  /// Mark the first coordinate of the next part.
  fn start_part(&mut self, _: WkbPartRole) {
    self.part_start = self.coordinates.len();
  }

  /// Append the decoded source vertex to the owned coordinate buffer.
  fn push_coord(&mut self, coordinate: Coord) {
    self.coordinates.push(coordinate);
  }

  /// Record the completed part length and count it in the active polygon.
  fn finish_part(&mut self) {
    self
      .lengths
      .push((self.coordinates.len() - self.part_start) as u32);
    if let Some(ring_count) = self.active_polygon_ring_count.as_mut() {
      *ring_count += 1;
    }
  }

  /// Record the completed polygon's ring count.
  fn finish_geometry(&mut self, ty: GeometryType) {
    if ty == GeometryType::Polygon {
      self.polygon_ring_counts.push(
        self
          .active_polygon_ring_count
          .take()
          .expect("started polygon geometry"),
      );
    }
  }
}

/// Tracks the byte position while reading WKB headers, nested members, and coordinates.
struct WkbReader<'a> {
  bytes: &'a [u8],
  offset: usize,
}

impl<'a> WkbReader<'a> {
  /// Start reading one geometry from the beginning of its WKB payload.
  fn new(bytes: &'a [u8]) -> Self {
    Self { bytes, offset: 0 }
  }

  /// Visit one geometry and its members through the shared part sink.
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
    sink.start_geometry(header.public.ty);
    match header.public.ty {
      GeometryType::Point => self.read_point_part(header, sink)?,
      GeometryType::LineString => self.read_line_string(header, WkbPartRole::Other, sink)?,
      GeometryType::Polygon => self.read_polygon(header, polygon_ring_order, sink)?,
      GeometryType::MultiPoint => self.read_multi_point(header, sink)?,
      GeometryType::MultiLineString => self.read_multi_line_string(header, sink)?,
      GeometryType::MultiPolygon => self.read_multi_polygon(header, polygon_ring_order, sink)?,
      GeometryType::GeometryCollection => {
        self.read_geometry_collection(header, polygon_ring_order, sink, depth + 1)?
      }
      GeometryType::Unknown => bail!("unsupported WKB geometry type"),
    }
    sink.finish_geometry(header.public.ty);
    Ok(header.public)
  }

  /// Read byte order, geometry type, and XY/XYZ/XYM/XYZM layout, skipping an EWKB SRID.
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
        ty: GeometryType::from_wkb_type(base_type),
        dimensions: dimensions(has_z, has_m),
      },
      byte_order,
      has_z,
      has_m,
    })
  }

  /// Visit one point as a single-coordinate part.
  fn read_point_part(&mut self, header: ParsedHeader, sink: &mut impl WkbSink) -> Result<()> {
    sink.start_part(WkbPartRole::Other);
    sink.push_coord(self.read_coordinate(header)?);
    sink.finish_part();
    Ok(())
  }

  /// Visit a counted coordinate sequence as one line or polygon ring.
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

  /// Visit polygon rings with exterior/interior roles in the requested vertex order.
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

  /// Visit a ring from its final vertex to its first, preserving each vertex's Z/M.
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

    // Preserve the forward reader position while visiting complete vertices in reverse.
    sink.start_part(role);
    for point_index in (0..point_count).rev() {
      let mut vertex_reader = Self {
        bytes: self.bytes,
        offset: ring_start + point_index * coordinate_width,
      };
      sink.push_coord(vertex_reader.read_coordinate(header)?);
    }
    sink.finish_part();
    self.offset = ring_end;
    Ok(())
  }

  /// Visit validated point members as one multipoint part.
  fn read_multi_point(&mut self, parent: ParsedHeader, sink: &mut impl WkbSink) -> Result<()> {
    let count = self.read_count(parent.byte_order, "point")?;
    sink.start_part(WkbPartRole::Other);
    for _ in 0..count {
      let header = self.read_header()?;
      require_type(header.public.ty, GeometryType::Point, "MultiPoint member")?;
      sink.push_coord(self.read_coordinate(header)?);
    }
    sink.finish_part();
    Ok(())
  }

  /// Visit validated line string members as separate parts.
  fn read_multi_line_string(
    &mut self,
    parent: ParsedHeader,
    sink: &mut impl WkbSink,
  ) -> Result<()> {
    let count = self.read_count(parent.byte_order, "line string")?;
    for _ in 0..count {
      let header = self.read_header()?;
      require_type(
        header.public.ty,
        GeometryType::LineString,
        "MultiLineString member",
      )?;
      self.read_line_string(header, WkbPartRole::Other, sink)?;
    }
    Ok(())
  }

  /// Visit validated polygon members while preserving their ring groups.
  fn read_multi_polygon(
    &mut self,
    parent: ParsedHeader,
    polygon_ring_order: PolygonRingOrder,
    sink: &mut impl WkbSink,
  ) -> Result<()> {
    let count = self.read_count(parent.byte_order, "polygon")?;
    for _ in 0..count {
      let header = self.read_header()?;
      require_type(
        header.public.ty,
        GeometryType::Polygon,
        "MultiPolygon member",
      )?;
      sink.start_geometry(GeometryType::Polygon);
      self.read_polygon(header, polygon_ring_order, sink)?;
      sink.finish_geometry(GeometryType::Polygon);
    }
    Ok(())
  }

  /// Visit nested collection members within the recursion limit.
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

  /// Write one geometry and its nested members with the selected Z/M components.
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
    write_iso_header(output, header.public.ty, output_has_z, output_has_m)?;
    match header.public.ty {
      GeometryType::Point => {
        let coordinate = self.read_coordinate(header)?;
        write_selected_coordinate(output, coordinate, output_has_z, output_has_m);
      }
      GeometryType::LineString => {
        self.write_stripped_coordinate_sequence(output, header, output_has_z, output_has_m)?;
      }
      GeometryType::Polygon => {
        let ring_count = self.read_count(header.byte_order, "ring")?;
        write_count(output, ring_count)?;
        for _ in 0..ring_count {
          self.write_stripped_coordinate_sequence(output, header, output_has_z, output_has_m)?;
        }
      }
      GeometryType::MultiPoint
      | GeometryType::MultiLineString
      | GeometryType::MultiPolygon
      | GeometryType::GeometryCollection => {
        let count = self.read_count(header.byte_order, "geometry")?;
        write_count(output, count)?;
        for _ in 0..count {
          self.write_stripped_geometry(output, strip_z, strip_m, depth + 1)?;
        }
      }
      GeometryType::Unknown => bail!("unsupported WKB geometry type"),
    }
    Ok(())
  }

  /// Write a counted coordinate sequence with the selected Z/M components.
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

  /// Read a WKB item count that fits the host index type.
  fn read_count(&mut self, byte_order: ByteOrder, item: &str) -> Result<usize> {
    usize::try_from(self.read_u32(byte_order)?)
      .map_err(|_| GeometryError::Wkb(format!("WKB {item} count does not fit usize")))
  }

  /// Read one vertex using the header's byte order and XY/XYZ/XYM/XYZM layout.
  fn read_coordinate(&mut self, header: ParsedHeader) -> Result<Coord> {
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
    Ok(Coord { x, y, z, m })
  }

  /// Read one byte and advance the cursor.
  fn read_u8(&mut self) -> Result<u8> {
    let value = *self.bytes.get(self.offset).ok_or_else(|| {
      GeometryError::Wkb(format!("unexpected end of WKB at byte {}", self.offset))
    })?;
    self.offset += 1;
    Ok(value)
  }

  /// Read one unsigned integer using the geometry's byte order.
  fn read_u32(&mut self, byte_order: ByteOrder) -> Result<u32> {
    let bytes = self.read_array::<4>()?;
    Ok(match byte_order {
      ByteOrder::BigEndian => u32::from_be_bytes(bytes),
      ByteOrder::LittleEndian => u32::from_le_bytes(bytes),
    })
  }

  /// Read one floating-point component using the geometry's byte order.
  fn read_f64(&mut self, byte_order: ByteOrder) -> Result<f64> {
    let bytes = self.read_array::<8>()?;
    Ok(match byte_order {
      ByteOrder::BigEndian => f64::from_be_bytes(bytes),
      ByteOrder::LittleEndian => f64::from_le_bytes(bytes),
    })
  }

  /// Read a fixed-width byte array, rejecting overflow and truncated input.
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

  /// Validate the complete byte range before visiting a reversed ring.
  fn require_end(&self, end: usize) -> Result<()> {
    if end > self.bytes.len() {
      bail!(
        "unexpected end of WKB: geometry requires {end} bytes but buffer has {}",
        self.bytes.len()
      );
    }
    Ok(())
  }

  /// Validate that the root geometry consumed the entire WKB payload.
  fn require_finished(&self) -> Result<()> {
    let remaining = self.bytes.len().saturating_sub(self.offset);
    if remaining != 0 {
      bail!("WKB contains {remaining} trailing bytes");
    }
    Ok(())
  }
}

#[derive(Debug, Clone, Copy)]
/// Stores the coordinate layout and byte order required to decode one geometry.
struct ParsedHeader {
  public: WkbHeader,
  byte_order: ByteOrder,
  has_z: bool,
  has_m: bool,
}

impl ParsedHeader {
  /// Resolve the number of stored XY, XYZ, XYM, or XYZM components.
  fn coordinate_width(self) -> usize {
    2 + usize::from(self.has_z) + usize::from(self.has_m)
  }
}

/// Tracks sequential part and coordinate positions for writing one quantized WKB geometry.
struct QuantizedWkbWriter<'a> {
  output: &'a mut Vec<u8>,
  geometry: &'a QuantizedGeometry,
  transform: &'a QuantizationTransform,
  part_index: usize,
  coordinate_offset: usize,
  stride: usize,
}

impl<'a> QuantizedWkbWriter<'a> {
  /// Start writing one quantized geometry with sequential part and coordinate cursors.
  fn new(
    output: &'a mut Vec<u8>,
    geometry: &'a QuantizedGeometry,
    transform: &'a QuantizationTransform,
  ) -> Self {
    Self {
      output,
      geometry,
      transform,
      part_index: 0,
      coordinate_offset: 0,
      stride: 2 + usize::from(geometry.has_z) + usize::from(geometry.has_m),
    }
  }

  /// Write the root WKB header and its point, part, or polygon payload.
  fn write_geometry(&mut self) -> Result<()> {
    let geometry = self.geometry;
    if matches!(
      geometry.ty,
      GeometryType::GeometryCollection | GeometryType::Unknown
    ) {
      bail!("unsupported quantized WKB geometry type {:?}", geometry.ty);
    }
    write_iso_header(self.output, geometry.ty, geometry.has_z, geometry.has_m)?;
    match geometry.ty {
      GeometryType::Point => self.write_coordinate()?,
      GeometryType::MultiPoint => {
        let point_count = geometry.coordinates.len() / self.stride;
        write_count(self.output, point_count)?;
        for _ in 0..point_count {
          write_iso_header(
            self.output,
            GeometryType::Point,
            geometry.has_z,
            geometry.has_m,
          )?;
          self.write_coordinate()?;
        }
      }
      GeometryType::LineString => self.write_part()?,
      GeometryType::MultiLineString => {
        write_count(self.output, geometry.lengths.len())?;
        for _ in &geometry.lengths {
          write_iso_header(
            self.output,
            GeometryType::LineString,
            geometry.has_z,
            geometry.has_m,
          )?;
          self.write_part()?;
        }
      }
      GeometryType::Polygon => {
        self.write_polygon(geometry.polygon_ring_counts.first().copied().unwrap_or(0))?;
      }
      GeometryType::MultiPolygon => {
        write_count(self.output, geometry.polygon_ring_counts.len())?;
        for &ring_count in &geometry.polygon_ring_counts {
          write_iso_header(
            self.output,
            GeometryType::Polygon,
            geometry.has_z,
            geometry.has_m,
          )?;
          self.write_polygon(ring_count)?;
        }
      }
      GeometryType::GeometryCollection | GeometryType::Unknown => {
        unreachable!("unsupported geometry type rejected before writing the header")
      }
    }
    Ok(())
  }

  /// Write the next polygon's ring count and consecutive ring parts.
  fn write_polygon(&mut self, ring_count: u32) -> Result<()> {
    write_count(self.output, ring_count as usize)?;
    for _ in 0..ring_count {
      self.write_part()?;
    }
    Ok(())
  }

  /// Write the next part and advance through its coordinates without rescanning prior lengths.
  fn write_part(&mut self) -> Result<()> {
    let point_count = *self.geometry.lengths.get(self.part_index).ok_or_else(|| {
      GeometryError::Wkb("quantized geometry part count does not match topology".to_string())
    })?;
    write_count(self.output, point_count as usize)?;
    for _ in 0..point_count {
      self.write_coordinate()?;
    }
    self.part_index += 1;
    Ok(())
  }

  /// Write the next quantized vertex as world-space XY and optional Z/M values.
  fn write_coordinate(&mut self) -> Result<()> {
    let coordinate_end = self
      .coordinate_offset
      .checked_add(self.stride)
      .ok_or_else(|| {
        GeometryError::Wkb("quantized coordinate offset overflows usize".to_string())
      })?;
    let coordinate = self
      .geometry
      .coordinates
      .get(self.coordinate_offset..coordinate_end)
      .ok_or_else(|| {
        GeometryError::Wkb("quantized coordinate does not match part lengths".to_string())
      })?;
    self
      .output
      .extend_from_slice(&self.transform.unquantize(coordinate[0], 0).to_le_bytes());
    self
      .output
      .extend_from_slice(&self.transform.unquantize(coordinate[1], 1).to_le_bytes());
    let mut component_index = 2;
    if self.geometry.has_z {
      let value = self.transform.unquantize(coordinate[component_index], 2);
      self.output.extend_from_slice(&value.to_le_bytes());
      component_index += 1;
    }
    if self.geometry.has_m {
      let value = self.transform.unquantize(coordinate[component_index], 3);
      self.output.extend_from_slice(&value.to_le_bytes());
    }
    self.coordinate_offset = coordinate_end;
    Ok(())
  }
}

/// Decode ISO layout offsets or EWKB flags into the base type and Z/M/SRID presence.
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
    _ => bail!("invalid ISO WKB XY/XYZ/XYM/XYZM type {encoded_type}"),
  };
  Ok((base_type, has_z, has_m, false))
}

/// Resolve the coordinate layout from the WKB Z/M flags.
fn dimensions(has_z: bool, has_m: bool) -> CoordinateDimensions {
  match (has_z, has_m) {
    (false, false) => CoordinateDimensions::Xy,
    (true, false) => CoordinateDimensions::Xyz,
    (true, true) => CoordinateDimensions::Xyzm,
    (false, true) => CoordinateDimensions::Xym,
  }
}

/// Write a little-endian ISO WKB header for the geometry type and selected Z/M.
fn write_iso_header(
  output: &mut Vec<u8>,
  ty: GeometryType,
  has_z: bool,
  has_m: bool,
) -> Result<()> {
  let base_type: u32 = match ty {
    GeometryType::Point => 1,
    GeometryType::LineString => 2,
    GeometryType::Polygon => 3,
    GeometryType::MultiPoint => 4,
    GeometryType::MultiLineString => 5,
    GeometryType::MultiPolygon => 6,
    GeometryType::GeometryCollection => 7,
    GeometryType::Unknown => bail!("unsupported WKB geometry type"),
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

/// Write a WKB item count, rejecting values beyond the wire format's u32 limit.
fn write_count(output: &mut Vec<u8>, count: usize) -> Result<()> {
  output.extend_from_slice(
    &u32::try_from(count)
      .map_err(|_| GeometryError::Wkb("WKB count exceeds u32".to_string()))?
      .to_le_bytes(),
  );
  Ok(())
}

/// Write a source vertex with only the selected Z/M components.
fn write_selected_coordinate(output: &mut Vec<u8>, coordinate: Coord, has_z: bool, has_m: bool) {
  output.extend_from_slice(&coordinate.x.to_le_bytes());
  output.extend_from_slice(&coordinate.y.to_le_bytes());
  if has_z {
    output.extend_from_slice(
      &coordinate
        .z
        .expect("selected Z originates from XYZ or XYZM WKB")
        .to_le_bytes(),
    );
  }
  if has_m {
    output.extend_from_slice(
      &coordinate
        .m
        .expect("selected M originates from XYM or XYZM WKB")
        .to_le_bytes(),
    );
  }
}

/// Validate the concrete type required by a homogeneous multipart geometry.
fn require_type(actual: GeometryType, expected: GeometryType, context: &str) -> Result<()> {
  if actual != expected {
    bail!("{context} must be {expected:?}, found {actual:?}");
  }
  Ok(())
}

#[cfg(test)]
/// Write a counted XY coordinate sequence for a WKB fixture.
fn write_test_coordinate_sequence(output: &mut Vec<u8>, coordinates: &[(f64, f64)]) {
  write_u32(output, coordinates.len());
  for &(x, y) in coordinates {
    write_coordinate(output, x, y);
  }
}

#[cfg(test)]
/// Write a little-endian XY WKB fixture header.
fn write_header(output: &mut Vec<u8>, geometry_type: u32) {
  output.push(1);
  output.extend_from_slice(&geometry_type.to_le_bytes());
}

#[cfg(test)]
/// Write a fixture count that fits the WKB u32 representation.
fn write_u32(output: &mut Vec<u8>, value: usize) {
  output.extend_from_slice(
    &u32::try_from(value)
      .expect("test geometry count fits u32")
      .to_le_bytes(),
  );
}

#[cfg(test)]
/// Write one XY fixture coordinate.
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
    coordinates: Vec<Coord>,
    lengths: Vec<usize>,
    current_length: usize,
  }

  impl WkbSink for CoordinateSink {
    /// Record the next part's role and reset its coordinate count.
    fn start_part(&mut self, role: WkbPartRole) {
      self.roles.push(role);
      self.current_length = 0;
    }

    /// Collect a decoded vertex and count it in the current part.
    fn push_coord(&mut self, coordinate: Coord) {
      self.coordinates.push(coordinate);
      self.current_length += 1;
    }

    /// Record the completed part's coordinate count.
    fn finish_part(&mut self) {
      self.lengths.push(self.current_length);
    }
  }

  mod header {
    use super::*;

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
          [Coord {
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
        Coord::from_point_wkb(&bytes).unwrap(),
        Coord {
          x: 1.0,
          y: 2.0,
          z: None,
          m: None,
        }
      );
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
  }

  mod decode {
    use super::*;

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
  }

  mod ring_order {
    use super::*;

    #[test]
    fn reverses_complete_vertices_in_each_layout_and_byte_order() {
      for dimension_code in 0..=3 {
        for little_endian in [true, false] {
          let has_z = matches!(dimension_code, 1 | 3);
          let has_m = matches!(dimension_code, 2 | 3);
          let source = [
            Coord {
              x: 1.0,
              y: 2.0,
              z: has_z.then_some(10.0),
              m: has_m.then_some(100.0),
            },
            Coord {
              x: 3.0,
              y: 4.0,
              z: has_z.then_some(20.0),
              m: has_m.then_some(200.0),
            },
            Coord {
              x: 1.0,
              y: 2.0,
              z: has_z.then_some(30.0),
              m: has_m.then_some(300.0),
            },
          ];
          let mut bytes = fixture_header(3, dimension_code);
          bytes.extend_from_slice(&1_u32.to_le_bytes());
          bytes.extend_from_slice(&3_u32.to_le_bytes());
          for coordinate in source {
            write_selected_coordinate(&mut bytes, coordinate, has_z, has_m);
          }
          if !little_endian {
            bytes[0] = 0;
            bytes[1..5].reverse();
            bytes[5..9].reverse();
            bytes[9..13].reverse();
            for component in bytes[13..].chunks_exact_mut(8) {
              component.reverse();
            }
          }
          let mut sink = CoordinateSink::default();

          visit_wkb_geometry(&bytes, PolygonRingOrder::Reverse, &mut sink).unwrap();

          assert_eq!(sink.coordinates, [source[2], source[1], source[0]]);
          assert_eq!(sink.lengths, [3]);
          assert_eq!(sink.roles, [WkbPartRole::Exterior]);
        }
      }
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
        Coord {
          x: 0.0,
          y: 0.0,
          z: Some(10.0),
          m: Some(100.0),
        },
        Coord {
          x: 2.0,
          y: 0.0,
          z: Some(20.0),
          m: Some(200.0),
        },
        Coord {
          x: 0.0,
          y: 2.0,
          z: Some(30.0),
          m: Some(300.0),
        },
        Coord {
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
          Coord {
            x: 0.0,
            y: 0.0,
            z: Some(10.0),
            m: Some(100.0),
          },
          Coord {
            x: 0.0,
            y: 2.0,
            z: Some(30.0),
            m: Some(300.0),
          },
          Coord {
            x: 2.0,
            y: 0.0,
            z: Some(20.0),
            m: Some(200.0),
          },
          Coord {
            x: 0.0,
            y: 0.0,
            z: Some(10.0),
            m: Some(100.0),
          },
        ]
      );
    }
  }

  mod strip {
    use super::*;

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
        let coordinate = Coord::from_point_wkb(&output).unwrap();

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
  }

  mod encode {
    use super::*;

    #[test]
    fn writes_each_geometry_type_in_each_coordinate_layout() {
      let transform = QuantizationTransform {
        scale: [1.0; 4],
        translate: [0.0; 4],
      };
      for dimension_code in 0..=3 {
        let has_z = matches!(dimension_code, 1 | 3);
        let has_m = matches!(dimension_code, 2 | 3);
        let mut coordinate = vec![1, 2];
        if has_z {
          coordinate.push(3);
        }
        if has_m {
          coordinate.push(4);
        }
        for (ty, base_type, lengths, polygon_ring_counts) in [
          (GeometryType::Point, 1, vec![1], vec![]),
          (GeometryType::LineString, 2, vec![3], vec![]),
          (GeometryType::Polygon, 3, vec![3, 4], vec![2]),
          (GeometryType::MultiPoint, 4, vec![2], vec![]),
          (GeometryType::MultiLineString, 5, vec![3, 2], vec![]),
          (GeometryType::MultiPolygon, 6, vec![3, 4, 3], vec![2, 1]),
        ] {
          let geometry = QuantizedGeometry {
            ty,
            coordinates: coordinate.repeat(lengths.iter().map(|length| *length as usize).sum()),
            lengths,
            polygon_ring_counts,
            has_z,
            has_m,
          };
          let mut expected = fixture_header(base_type, dimension_code);
          match ty {
            GeometryType::Point => write_fixture_coordinate(&mut expected, dimension_code),
            GeometryType::LineString => expected = line_string_fixture(dimension_code, 3),
            GeometryType::Polygon => expected = polygon_fixture(dimension_code, &[3, 4]),
            GeometryType::MultiPoint => {
              expected.extend_from_slice(&2_u32.to_le_bytes());
              expected.extend_from_slice(&point_fixture(dimension_code));
              expected.extend_from_slice(&point_fixture(dimension_code));
            }
            GeometryType::MultiLineString => {
              expected.extend_from_slice(&2_u32.to_le_bytes());
              expected.extend_from_slice(&line_string_fixture(dimension_code, 3));
              expected.extend_from_slice(&line_string_fixture(dimension_code, 2));
            }
            GeometryType::MultiPolygon => {
              expected.extend_from_slice(&2_u32.to_le_bytes());
              expected.extend_from_slice(&polygon_fixture(dimension_code, &[3, 4]));
              expected.extend_from_slice(&polygon_fixture(dimension_code, &[3]));
            }
            _ => unreachable!(),
          }
          let mut builder = WkbArrayBuilder::new(1);

          builder.append(&geometry, &transform).unwrap();
          let array = builder.finish();
          let array = array
            .as_any()
            .downcast_ref::<arrow_array::BinaryArray>()
            .unwrap();

          assert_eq!(array.value(0), expected, "{ty:?}, layout {dimension_code}");
        }
      }
    }

    #[test]
    fn advances_coordinates_across_unequal_and_empty_parts() {
      let transform = QuantizationTransform {
        scale: [0.5, 2.0, 3.0, 4.0],
        translate: [10.0, 20.0, 30.0, 40.0],
      };
      let coordinates = vec![
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
      ];
      for ty in [GeometryType::MultiLineString, GeometryType::MultiPolygon] {
        let geometry = QuantizedGeometry {
          ty,
          coordinates: coordinates.clone(),
          lengths: vec![0, 2, 1, 0, 3],
          polygon_ring_counts: if ty == GeometryType::MultiPolygon {
            vec![2, 0, 3]
          } else {
            vec![]
          },
          has_z: true,
          has_m: true,
        };
        let mut builder = WkbArrayBuilder::new(1);
        builder.append(&geometry, &transform).unwrap();
        let array = builder.finish();
        let array = array
          .as_any()
          .downcast_ref::<arrow_array::BinaryArray>()
          .unwrap();

        let decoded = Geometry::from_wkb(array.value(0)).unwrap();

        assert_eq!(decoded.lengths, geometry.lengths);
        assert_eq!(decoded.polygon_ring_counts, geometry.polygon_ring_counts);
        for (index, coordinate) in decoded.coordinates.iter().enumerate() {
          let offset = (index * 4) as f64;
          assert_eq!(
            *coordinate,
            Coord {
              x: offset * 0.5 + 10.0,
              y: (offset + 1.0) * 2.0 + 20.0,
              z: Some((offset + 2.0) * 3.0 + 30.0),
              m: Some((offset + 3.0) * 4.0 + 40.0),
            }
          );
        }
        assert_eq!(geometry.coordinates, coordinates);
      }
    }

    #[test]
    fn reuses_buffer_across_layouts_and_null_rows() {
      use arrow_array::Array;

      let transform = QuantizationTransform {
        scale: [1.0; 4],
        translate: [0.0; 4],
      };
      let mut geometry = QuantizedGeometry {
        ty: GeometryType::LineString,
        coordinates: vec![1, 2, 3, 4, 1, 2, 3, 4],
        lengths: vec![2],
        polygon_ring_counts: vec![],
        has_z: true,
        has_m: true,
      };
      let mut builder = WkbArrayBuilder::new(3);
      builder.append(&geometry, &transform).unwrap();
      builder.append_null();
      assert_eq!(geometry.coordinates, [1, 2, 3, 4, 1, 2, 3, 4]);
      geometry.ty = GeometryType::Point;
      geometry.coordinates = vec![1, 2];
      geometry.lengths = vec![1];
      geometry.has_z = false;
      geometry.has_m = false;
      builder.append(&geometry, &transform).unwrap();

      let array = builder.finish();
      let array = array
        .as_any()
        .downcast_ref::<arrow_array::BinaryArray>()
        .unwrap();

      assert_eq!(array.len(), 3);
      assert_eq!(array.value(0), line_string_fixture(3, 2));
      assert!(array.is_null(1));
      assert_eq!(array.value(2), point_fixture(0));
      assert_eq!(geometry.coordinates, [1, 2]);
    }

    #[test]
    fn writes_transformed_multipolygon_wkb_with_ring_groups() {
      let geometry = QuantizedGeometry {
        ty: GeometryType::MultiPolygon,
        coordinates: vec![0, 0, 2, 0, 2, 2, 0, 0, 4, 4, 6, 4, 6, 6, 4, 4],
        lengths: vec![4, 4],
        polygon_ring_counts: vec![1, 1],
        has_z: false,
        has_m: false,
      };
      let transform = QuantizationTransform {
        scale: [0.5, 0.5, 1.0, 1.0],
        translate: [10.0, 20.0, 0.0, 0.0],
      };
      let mut bytes = Vec::new();

      QuantizedWkbWriter::new(&mut bytes, &geometry, &transform)
        .write_geometry()
        .unwrap();
      let decoded = Geometry::from_wkb(&bytes).unwrap();

      assert_eq!(decoded.ty, GeometryType::MultiPolygon);
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
  }

  mod validation {
    use super::*;

    #[test]
    fn rejects_trailing_bytes_during_visiting_and_stripping() {
      let mut bytes = point_fixture(3);
      bytes.push(0);
      let mut sink = CoordinateSink::default();

      for error in [
        visit_wkb_geometry(&bytes, PolygonRingOrder::Preserve, &mut sink).unwrap_err(),
        strip_wkb_dimensions(&bytes, true, false).unwrap_err(),
      ] {
        assert!(error.to_string().contains("WKB contains 1 trailing bytes"));
      }
    }

    #[test]
    fn rejects_truncated_reversed_rings_before_visiting_vertices() {
      let mut bytes = polygon_fixture(3, &[3]);
      bytes.pop();
      let mut sink = CoordinateSink::default();

      let error = visit_wkb_geometry(&bytes, PolygonRingOrder::Reverse, &mut sink).unwrap_err();

      assert!(error.to_string().contains("unexpected end of WKB"));
      assert!(sink.coordinates.is_empty());
    }

    #[test]
    fn rejects_inconsistent_quantized_parts_without_appending_a_row() {
      let transform = QuantizationTransform {
        scale: [1.0; 4],
        translate: [0.0; 4],
      };
      let mut builder = WkbArrayBuilder::new(1);
      for (ty, lengths, polygon_ring_counts, expected_error) in [
        (
          GeometryType::LineString,
          vec![2],
          vec![],
          "coordinate does not match part lengths",
        ),
        (
          GeometryType::Polygon,
          vec![],
          vec![1],
          "part count does not match topology",
        ),
        (
          GeometryType::GeometryCollection,
          vec![1],
          vec![],
          "unsupported quantized WKB geometry type",
        ),
      ] {
        let geometry = QuantizedGeometry {
          ty,
          coordinates: vec![1, 2],
          lengths,
          polygon_ring_counts,
          has_z: false,
          has_m: false,
        };

        let error = builder.append(&geometry, &transform).unwrap_err();

        assert!(error.to_string().contains(expected_error));
      }
      builder.append_null();
      assert_eq!(builder.finish().len(), 1);
    }

    #[test]
    fn rejects_truncated_geometry_without_panicking() {
      let error = Coord::from_point_wkb(&[1, 1, 0, 0, 0]).unwrap_err();
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
  }

  /// Collect fixture parts in source order.
  fn decode_fixture(bytes: &[u8]) -> CoordinateSink {
    let mut sink = CoordinateSink::default();
    visit_wkb_geometry(bytes, PolygonRingOrder::Preserve, &mut sink).unwrap();
    sink
  }

  /// Build a point fixture in one ISO coordinate layout.
  fn point_fixture(dimension_code: u32) -> Vec<u8> {
    let mut bytes = fixture_header(1, dimension_code);
    write_fixture_coordinate(&mut bytes, dimension_code);
    bytes
  }

  /// Build a line string fixture with the requested vertex count and ISO layout.
  fn line_string_fixture(dimension_code: u32, point_count: u32) -> Vec<u8> {
    let mut bytes = fixture_header(2, dimension_code);
    bytes.extend_from_slice(&point_count.to_le_bytes());
    for _ in 0..point_count {
      write_fixture_coordinate(&mut bytes, dimension_code);
    }
    bytes
  }

  /// Build a polygon fixture with the requested ring lengths and ISO layout.
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

  /// Build an independent little-endian ISO WKB fixture header.
  fn fixture_header(base_type: u32, dimension_code: u32) -> Vec<u8> {
    let mut bytes = vec![1];
    bytes.extend_from_slice(&(dimension_code * 1000 + base_type).to_le_bytes());
    bytes
  }

  /// Write fixture XY values and the ISO layout's optional Z/M components.
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

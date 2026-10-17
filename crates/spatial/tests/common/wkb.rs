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

pub type XyzmCoordinate = (f64, f64, Option<f64>, Option<f64>);

/// Encode an XY point with little-endian WKB coordinates.
pub fn point(x: f64, y: f64) -> Vec<u8> {
  let mut output = Vec::with_capacity(21);
  header(&mut output, 1);
  coordinate(&mut output, x, y);
  output
}

/// Encode an XY, XYZ, XYM, or XYZM point according to its present components.
pub fn xyzm_point(x: f64, y: f64, z: Option<f64>, m: Option<f64>) -> Vec<u8> {
  let mut output = Vec::with_capacity(37);
  header(&mut output, xyzm_type(1, z.is_some(), m.is_some()));
  xyzm_coordinate(&mut output, x, y, z, m);
  output
}

/// Encode a multipoint with shared layout flags and a WKB header for each point.
pub fn xyzm_multi_point(coordinates: &[XyzmCoordinate]) -> Vec<u8> {
  let (has_z, has_m) = dimensions(coordinates);
  let coordinate_size = (2 + usize::from(has_z) + usize::from(has_m)) * 8;
  let mut output = Vec::with_capacity(9 + coordinates.len() * (5 + coordinate_size));
  header(&mut output, xyzm_type(4, has_z, has_m));
  coordinate_count(&mut output, coordinates.len());
  for &(x, y, z, m) in coordinates {
    header(&mut output, xyzm_type(1, has_z, has_m));
    xyzm_coordinate(&mut output, x, y, z, m);
  }
  output
}

/// Encode a line string with layout flags inferred from its optional Z/M components.
pub fn xyzm_line_string(coordinates: &[XyzmCoordinate]) -> Vec<u8> {
  let (has_z, has_m) = dimensions(coordinates);
  let coordinate_size = (2 + usize::from(has_z) + usize::from(has_m)) * 8;
  let mut output = Vec::with_capacity(9 + coordinates.len() * coordinate_size);
  header(&mut output, xyzm_type(2, has_z, has_m));
  coordinate_count(&mut output, coordinates.len());
  for &(x, y, z, m) in coordinates {
    xyzm_coordinate(&mut output, x, y, z, m);
  }
  output
}

/// Encode a single-ring XY polygon without changing coordinate order or closure.
pub fn polygon(coordinates: &[(f64, f64)]) -> Vec<u8> {
  let mut output = Vec::with_capacity(13 + coordinates.len() * 16);
  header(&mut output, 3);
  output.extend_from_slice(&1_u32.to_le_bytes());
  output.extend_from_slice(
    &u32::try_from(coordinates.len())
      .expect("fixture coordinate count fits u32")
      .to_le_bytes(),
  );
  for &(x, y) in coordinates {
    coordinate(&mut output, x, y);
  }
  output
}

/// Encode a polygon from one ring with its supplied optional Z/M components.
pub fn xyzm_polygon(coordinates: &[(f64, f64, Option<f64>, Option<f64>)]) -> Vec<u8> {
  xyzm_polygon_rings(&[coordinates])
}

/// Encode exterior and interior rings with layout flags shared across the polygon.
pub fn xyzm_polygon_rings(rings: &[&[XyzmCoordinate]]) -> Vec<u8> {
  let mut has_z = false;
  let mut has_m = false;
  let mut total_coordinate_count = 0;
  for ring in rings {
    let (ring_has_z, ring_has_m) = dimensions(ring);
    has_z |= ring_has_z;
    has_m |= ring_has_m;
    total_coordinate_count += ring.len();
  }
  let coordinate_size = (2 + usize::from(has_z) + usize::from(has_m)) * 8;
  let mut output =
    Vec::with_capacity(9 + rings.len() * 4 + total_coordinate_count * coordinate_size);
  header(&mut output, xyzm_type(3, has_z, has_m));
  coordinate_count(&mut output, rings.len());
  for ring in rings {
    coordinate_count(&mut output, ring.len());
    for &(x, y, z, m) in *ring {
      xyzm_coordinate(&mut output, x, y, z, m);
    }
  }
  output
}

/// Read XY coordinates after requiring a little-endian point WKB header.
pub fn read_point(bytes: &[u8]) -> Result<(f64, f64), String> {
  require_little_endian_type(bytes, 1)?;
  Ok((read_f64(bytes, 5)?, read_f64(bytes, 13)?))
}

/// Read exterior-ring XY bounds while rejecting missing rings and truncated coordinates.
pub fn read_polygon_extent(bytes: &[u8]) -> Result<[f64; 4], String> {
  require_little_endian_type(bytes, 3)?;
  let ring_count = read_u32(bytes, 5)?;
  if ring_count == 0 {
    return Err("polygon missing exterior ring".to_string());
  }
  let point_count = usize::try_from(read_u32(bytes, 9)?).map_err(|error| error.to_string())?;
  let mut extent = [
    f64::INFINITY,
    f64::INFINITY,
    f64::NEG_INFINITY,
    f64::NEG_INFINITY,
  ];
  for point_index in 0..point_count {
    let coordinate_offset = 13 + point_index * 16;
    let x = read_f64(bytes, coordinate_offset)?;
    let y = read_f64(bytes, coordinate_offset + 8)?;
    extent[0] = extent[0].min(x);
    extent[1] = extent[1].min(y);
    extent[2] = extent[2].max(x);
    extent[3] = extent[3].max(y);
  }
  Ok(extent)
}

/// Append the little-endian marker and ISO WKB geometry type.
fn header(output: &mut Vec<u8>, geometry_type: u32) {
  output.push(1);
  output.extend_from_slice(&geometry_type.to_le_bytes());
}

/// Append one XY pair as little-endian Float64 components.
fn coordinate(output: &mut Vec<u8>, x: f64, y: f64) {
  output.extend_from_slice(&x.to_le_bytes());
  output.extend_from_slice(&y.to_le_bytes());
}

/// Append a fixture part or coordinate count after requiring it to fit UInt32.
fn coordinate_count(output: &mut Vec<u8>, count: usize) {
  output.extend_from_slice(
    &u32::try_from(count)
      .expect("fixture coordinate count fits u32")
      .to_le_bytes(),
  );
}

/// Append XY followed by each present Z/M component without filling absent values.
fn xyzm_coordinate(output: &mut Vec<u8>, x: f64, y: f64, z: Option<f64>, m: Option<f64>) {
  coordinate(output, x, y);
  if let Some(z) = z {
    output.extend_from_slice(&z.to_le_bytes());
  }
  if let Some(m) = m {
    output.extend_from_slice(&m.to_le_bytes());
  }
}

/// Detect whether any fixture coordinate supplies a Z component or an M component.
fn dimensions(coordinates: &[XyzmCoordinate]) -> (bool, bool) {
  (
    coordinates.iter().any(|coordinate| coordinate.2.is_some()),
    coordinates.iter().any(|coordinate| coordinate.3.is_some()),
  )
}

/// Apply the ISO WKB type offset for XY, XYZ, XYM, or XYZM layout.
fn xyzm_type(base_type: u32, has_z: bool, has_m: bool) -> u32 {
  base_type
    + match (has_z, has_m) {
      (false, false) => 0,
      (true, false) => 1000,
      (false, true) => 2000,
      (true, true) => 3000,
    }
}

/// Reject fixture WKB with an unexpected byte order, geometry type, or truncated header.
fn require_little_endian_type(bytes: &[u8], expected_type: u32) -> Result<(), String> {
  if bytes.first() != Some(&1) {
    return Err("expected little-endian WKB".to_string());
  }
  let geometry_type = read_u32(bytes, 1)?;
  if geometry_type != expected_type {
    return Err(format!(
      "expected WKB type {expected_type}, found {geometry_type}"
    ));
  }
  Ok(())
}

/// Read a little-endian UInt32 at the requested offset or report truncated WKB.
fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
  let value = bytes
    .get(offset..offset + 4)
    .ok_or_else(|| "truncated WKB".to_string())?
    .try_into()
    .expect("slice length matches array length");
  Ok(u32::from_le_bytes(value))
}

/// Read a little-endian Float64 at the requested offset or report truncated WKB.
fn read_f64(bytes: &[u8], offset: usize) -> Result<f64, String> {
  let value = bytes
    .get(offset..offset + 8)
    .ok_or_else(|| "truncated WKB".to_string())?
    .try_into()
    .expect("slice length matches array length");
  Ok(f64::from_le_bytes(value))
}

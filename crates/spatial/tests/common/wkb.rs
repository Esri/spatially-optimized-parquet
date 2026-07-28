pub type DimensionalCoordinate = (f64, f64, Option<f64>, Option<f64>);

pub fn point(x: f64, y: f64) -> Vec<u8> {
  let mut output = Vec::with_capacity(21);
  header(&mut output, 1);
  coordinate(&mut output, x, y);
  output
}

pub fn dimensional_point(x: f64, y: f64, z: Option<f64>, m: Option<f64>) -> Vec<u8> {
  let mut output = Vec::with_capacity(37);
  header(&mut output, dimensional_type(1, z.is_some(), m.is_some()));
  dimensional_coordinate(&mut output, x, y, z, m);
  output
}

pub fn dimensional_multi_point(coordinates: &[DimensionalCoordinate]) -> Vec<u8> {
  let (has_z, has_m) = dimensions(coordinates);
  let coordinate_size = (2 + usize::from(has_z) + usize::from(has_m)) * 8;
  let mut output = Vec::with_capacity(9 + coordinates.len() * (5 + coordinate_size));
  header(&mut output, dimensional_type(4, has_z, has_m));
  coordinate_count(&mut output, coordinates.len());
  for &(x, y, z, m) in coordinates {
    header(&mut output, dimensional_type(1, has_z, has_m));
    dimensional_coordinate(&mut output, x, y, z, m);
  }
  output
}

pub fn dimensional_line_string(coordinates: &[DimensionalCoordinate]) -> Vec<u8> {
  let (has_z, has_m) = dimensions(coordinates);
  let coordinate_size = (2 + usize::from(has_z) + usize::from(has_m)) * 8;
  let mut output = Vec::with_capacity(9 + coordinates.len() * coordinate_size);
  header(&mut output, dimensional_type(2, has_z, has_m));
  coordinate_count(&mut output, coordinates.len());
  for &(x, y, z, m) in coordinates {
    dimensional_coordinate(&mut output, x, y, z, m);
  }
  output
}

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

pub fn dimensional_polygon(coordinates: &[(f64, f64, Option<f64>, Option<f64>)]) -> Vec<u8> {
  dimensional_polygon_rings(&[coordinates])
}

pub fn dimensional_polygon_rings(rings: &[&[DimensionalCoordinate]]) -> Vec<u8> {
  let coordinates = rings
    .iter()
    .flat_map(|ring| ring.iter())
    .copied()
    .collect::<Vec<_>>();
  let (has_z, has_m) = dimensions(&coordinates);
  let coordinate_size = (2 + usize::from(has_z) + usize::from(has_m)) * 8;
  let total_coordinate_count = coordinates.len();
  let mut output =
    Vec::with_capacity(9 + rings.len() * 4 + total_coordinate_count * coordinate_size);
  header(&mut output, dimensional_type(3, has_z, has_m));
  coordinate_count(&mut output, rings.len());
  for ring in rings {
    coordinate_count(&mut output, ring.len());
    for &(x, y, z, m) in *ring {
      dimensional_coordinate(&mut output, x, y, z, m);
    }
  }
  output
}

pub fn read_point(bytes: &[u8]) -> Result<(f64, f64), String> {
  require_little_endian_type(bytes, 1)?;
  Ok((read_f64(bytes, 5)?, read_f64(bytes, 13)?))
}

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

fn header(output: &mut Vec<u8>, geometry_type: u32) {
  output.push(1);
  output.extend_from_slice(&geometry_type.to_le_bytes());
}

fn coordinate(output: &mut Vec<u8>, x: f64, y: f64) {
  output.extend_from_slice(&x.to_le_bytes());
  output.extend_from_slice(&y.to_le_bytes());
}

fn coordinate_count(output: &mut Vec<u8>, count: usize) {
  output.extend_from_slice(
    &u32::try_from(count)
      .expect("fixture coordinate count fits u32")
      .to_le_bytes(),
  );
}

fn dimensional_coordinate(output: &mut Vec<u8>, x: f64, y: f64, z: Option<f64>, m: Option<f64>) {
  coordinate(output, x, y);
  if let Some(z) = z {
    output.extend_from_slice(&z.to_le_bytes());
  }
  if let Some(m) = m {
    output.extend_from_slice(&m.to_le_bytes());
  }
}

fn dimensions(coordinates: &[DimensionalCoordinate]) -> (bool, bool) {
  (
    coordinates.iter().any(|coordinate| coordinate.2.is_some()),
    coordinates.iter().any(|coordinate| coordinate.3.is_some()),
  )
}

fn dimensional_type(base_type: u32, has_z: bool, has_m: bool) -> u32 {
  base_type
    + match (has_z, has_m) {
      (false, false) => 0,
      (true, false) => 1000,
      (false, true) => 2000,
      (true, true) => 3000,
    }
}

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

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
  let value = bytes
    .get(offset..offset + 4)
    .ok_or_else(|| "truncated WKB".to_string())?
    .try_into()
    .expect("slice length matches array length");
  Ok(u32::from_le_bytes(value))
}

fn read_f64(bytes: &[u8], offset: usize) -> Result<f64, String> {
  let value = bytes
    .get(offset..offset + 8)
    .ok_or_else(|| "truncated WKB".to_string())?
    .try_into()
    .expect("slice length matches array length");
  Ok(f64::from_le_bytes(value))
}

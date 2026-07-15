use anyhow::{Result, bail};

pub fn point(x: f64, y: f64) -> Vec<u8> {
  let mut output = Vec::with_capacity(21);
  header(&mut output, 1);
  coordinate(&mut output, x, y);
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

pub fn read_point(bytes: &[u8]) -> Result<(f64, f64)> {
  require_little_endian_type(bytes, 1)?;
  Ok((read_f64(bytes, 5)?, read_f64(bytes, 13)?))
}

pub fn read_polygon_extent(bytes: &[u8]) -> Result<[f64; 4]> {
  require_little_endian_type(bytes, 3)?;
  let ring_count = read_u32(bytes, 5)?;
  if ring_count == 0 {
    bail!("polygon missing exterior ring");
  }
  let point_count = usize::try_from(read_u32(bytes, 9)?)?;
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

fn require_little_endian_type(bytes: &[u8], expected_type: u32) -> Result<()> {
  if bytes.first() != Some(&1) {
    bail!("expected little-endian WKB");
  }
  let geometry_type = read_u32(bytes, 1)?;
  if geometry_type != expected_type {
    bail!("expected WKB type {expected_type}, found {geometry_type}");
  }
  Ok(())
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
  let value = bytes
    .get(offset..offset + 4)
    .ok_or_else(|| anyhow::anyhow!("truncated WKB"))?
    .try_into()
    .expect("slice length matches array length");
  Ok(u32::from_le_bytes(value))
}

fn read_f64(bytes: &[u8], offset: usize) -> Result<f64> {
  let value = bytes
    .get(offset..offset + 8)
    .ok_or_else(|| anyhow::anyhow!("truncated WKB"))?
    .try_into()
    .expect("slice length matches array length");
  Ok(f64::from_le_bytes(value))
}

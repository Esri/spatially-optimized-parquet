//! Quantizes and simplifies flat geometry into delta-encoded coordinate vectors.

use anyhow::{Result, bail};

use super::GeometryEncoding;

pub(super) fn encode_quantized_payload_into(
  input_coords: &[f64],
  input_lengths: &[u32],
  encoding: &GeometryEncoding,
  coords: &mut Vec<i64>,
  lengths: &mut Vec<u32>,
) -> Result<()> {
  coords.clear();
  lengths.clear();
  let mut offset = 0usize;
  let mut degenerated_coordinate = None;

  for &length in input_lengths {
    let point_count = length as usize;
    if point_count == 0 {
      continue;
    }

    let part_start = coords.len();
    let part = &input_coords[offset..offset + point_count * 2];
    let mut vertices = part.chunks_exact(2);
    let first = vertices
      .next()
      .expect("non-empty part should have a first vertex");
    let mut previous_x = quantize(
      first[0],
      encoding.transform.scale[0],
      encoding.transform.translate[0],
    )?;
    let mut previous_y = quantize(
      first[1],
      encoding.transform.scale[1],
      encoding.transform.translate[1],
    )?;
    coords.push(previous_x);
    coords.push(previous_y);
    let mut output_length = 1u32;
    let mut previous_dx = 0i64;
    let mut previous_dy = 0i64;

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

      if x == previous_x && y == previous_y {
        continue;
      }

      let dx = x - previous_x;
      let dy = y - previous_y;
      if is_collinear_delta(previous_dx, previous_dy, dx, dy) {
        let coordinate_count = coords.len();
        coords[coordinate_count - 2] += dx;
        coords[coordinate_count - 1] += dy;
        previous_x += dx;
        previous_y += dy;
      } else {
        coords.push(dx);
        coords.push(dy);
        previous_x = x;
        previous_y = y;
        previous_dx = dx;
        previous_dy = dy;
        output_length += 1;
      }
    }

    if output_length < encoding.min_length as u32 {
      degenerated_coordinate.get_or_insert((coords[part_start], coords[part_start + 1]));
      coords.truncate(part_start);
    } else {
      lengths.push(output_length);
    }
    offset += point_count * 2;
  }

  if lengths.is_empty()
    && let Some((x, y)) = degenerated_coordinate
  {
    coords.extend([x, y]);
    lengths.push(1);
  }

  Ok(())
}

fn quantize(value: f64, scale: f64, translate: f64) -> Result<i64> {
  let normalized = ((value - translate) / scale).round();
  if !normalized.is_finite() || normalized < i64::MIN as f64 || normalized > i64::MAX as f64 {
    bail!("quantized coordinate out of range")
  }
  Ok(normalized as i64)
}

fn is_collinear_delta(previous_dx: i64, previous_dy: i64, dx: i64, dy: i64) -> bool {
  previous_dx * dy == dx * previous_dy && (previous_dx * dx + previous_dy * dy) > 0
}

#[cfg(test)]
mod tests {
  use super::super::levels::QuantizationTransform;

  use super::*;

  fn test_encoding(min_length: usize) -> GeometryEncoding {
    GeometryEncoding {
      level: 0,
      column: "level_0".to_string(),
      resolution: 1.0,
      scale: 1.0,
      transform: QuantizationTransform {
        scale: [1.0, 1.0, 1.0, 1.0],
        translate: [0.0, 0.0, 0.0, 0.0],
      },
      min_length,
    }
  }

  #[test]
  fn merges_collinear_lines() {
    let mut coords = Vec::new();
    let mut lengths = Vec::new();
    encode_quantized_payload_into(
      &[0.0, 0.0, 1.0, 1.0, 2.0, 2.0],
      &[3],
      &test_encoding(2),
      &mut coords,
      &mut lengths,
    )
    .unwrap();
    assert_eq!(lengths, vec![2]);
    assert_eq!(coords, vec![0, 0, 2, 2]);
  }

  #[test]
  fn drops_duplicate_snapped_vertices() {
    let mut coords = Vec::new();
    let mut lengths = Vec::new();
    encode_quantized_payload_into(
      &[0.1, 0.1, 0.2, 0.2, 0.9, 0.9, 1.0, 1.0],
      &[4],
      &test_encoding(2),
      &mut coords,
      &mut lengths,
    )
    .unwrap();
    assert_eq!(lengths, vec![2]);
    assert_eq!(coords, vec![0, 0, 1, 1]);
  }

  #[test]
  fn preserves_one_coordinate_for_degenerated_geometry() {
    let mut coords = Vec::new();
    let mut lengths = Vec::new();
    encode_quantized_payload_into(
      &[0.1, 0.1, 0.2, 0.2],
      &[2],
      &test_encoding(2),
      &mut coords,
      &mut lengths,
    )
    .unwrap();
    assert_eq!(lengths, vec![1]);
    assert_eq!(coords, vec![0, 0]);
  }
}

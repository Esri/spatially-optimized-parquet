//! Quantizes and simplifies flat geometry into delta-encoded coordinate vectors.

use anyhow::{Result, bail};

use super::GeometryEncoding;
use crate::geometry::WkbCoordinate;

pub(super) fn encode_quantized_payload_into(
  input_coordinates: &[WkbCoordinate],
  input_lengths: &[u32],
  encoding: &GeometryEncoding,
  has_z: bool,
  has_m: bool,
  coords: &mut Vec<i64>,
  lengths: &mut Vec<u32>,
) -> Result<()> {
  coords.clear();
  lengths.clear();
  let mut offset = 0usize;
  let mut degenerated_coordinate = None::<Vec<i64>>;

  for &length in input_lengths {
    let point_count = length as usize;
    if point_count == 0 {
      continue;
    }

    let part_start = coords.len();
    let part = &input_coordinates[offset..offset + point_count];
    let output_length = if has_z || has_m {
      encode_dimensional_part(part, encoding, has_z, has_m, coords)?
    } else {
      encode_xy_part(part, encoding, coords)?
    };

    if output_length < encoding.min_length as u32 {
      degenerated_coordinate.get_or_insert_with(|| {
        coords[part_start..part_start + coordinate_stride(has_z, has_m)].to_vec()
      });
      coords.truncate(part_start);
    } else {
      lengths.push(output_length);
    }
    offset += point_count;
  }

  if lengths.is_empty()
    && let Some(coordinate) = degenerated_coordinate
  {
    coords.extend(coordinate);
    lengths.push(1);
  }

  fn encode_xy_part(
    part: &[WkbCoordinate],
    encoding: &GeometryEncoding,
    coords: &mut Vec<i64>,
  ) -> Result<u32> {
    let first = part.first().expect("non-empty part");
    let mut previous_x = quantize_axis(first.x, encoding, 0)?;
    let mut previous_y = quantize_axis(first.y, encoding, 1)?;
    coords.extend([previous_x, previous_y]);
    let mut output_length = 1u32;
    let mut previous_dx = 0i64;
    let mut previous_dy = 0i64;

    for coordinate in &part[1..] {
      let x = quantize_axis(coordinate.x, encoding, 0)?;
      let y = quantize_axis(coordinate.y, encoding, 1)?;
      if x == previous_x && y == previous_y {
        continue;
      }
      let dx = x - previous_x;
      let dy = y - previous_y;
      if is_collinear_delta(previous_dx, previous_dy, dx, dy) {
        let coordinate_count = coords.len();
        coords[coordinate_count - 2] += dx;
        coords[coordinate_count - 1] += dy;
        previous_x = x;
        previous_y = y;
      } else {
        coords.extend([dx, dy]);
        previous_x = x;
        previous_y = y;
        previous_dx = dx;
        previous_dy = dy;
        output_length += 1;
      }
    }
    Ok(output_length)
  }

  fn encode_dimensional_part(
    part: &[WkbCoordinate],
    encoding: &GeometryEncoding,
    has_z: bool,
    has_m: bool,
    coords: &mut Vec<i64>,
  ) -> Result<u32> {
    let retained = douglas_peucker_indices(part, encoding.resolution);
    let mut previous_x = 0;
    let mut previous_y = 0;
    for (output_index, input_index) in retained.iter().copied().enumerate() {
      let coordinate = part[input_index];
      let x = quantize_axis(coordinate.x, encoding, 0)?;
      let y = quantize_axis(coordinate.y, encoding, 1)?;
      if output_index == 0 {
        coords.extend([x, y]);
      } else {
        coords.extend([x - previous_x, y - previous_y]);
      }
      previous_x = x;
      previous_y = y;
      if has_z {
        coords.push(quantize_ordinate(
          coordinate.z,
          encoding.transform.scale[2],
          encoding.transform.translate[2],
        )?);
      }
      if has_m {
        coords.push(quantize_ordinate(
          coordinate.m,
          encoding.transform.scale[3],
          encoding.transform.translate[3],
        )?);
      }
    }
    Ok(retained.len() as u32)
  }

  fn douglas_peucker_indices(coordinates: &[WkbCoordinate], tolerance: f64) -> Vec<usize> {
    if coordinates.len() <= 2 {
      return (0..coordinates.len()).collect();
    }
    let closed = same_xy(
      coordinates[0],
      *coordinates.last().expect("non-empty coordinates"),
    );
    if !closed {
      return simplify_open_indices(
        coordinates,
        &(0..coordinates.len()).collect::<Vec<_>>(),
        tolerance,
      );
    }

    let unique_count = coordinates.len() - 1;
    if unique_count <= 3 {
      return (0..coordinates.len()).collect();
    }
    let opposite = (1..unique_count)
      .max_by(|left, right| {
        squared_distance(coordinates[0], coordinates[*left])
          .total_cmp(&squared_distance(coordinates[0], coordinates[*right]))
      })
      .expect("closed ring has another vertex");
    let first_arc = (0..=opposite).collect::<Vec<_>>();
    let mut second_arc = (opposite..unique_count).collect::<Vec<_>>();
    second_arc.push(0);
    let mut retained = simplify_open_indices(coordinates, &first_arc, tolerance);
    let second = simplify_open_indices(coordinates, &second_arc, tolerance);
    retained.extend(second.into_iter().skip(1).filter(|index| *index != 0));
    retained.sort_unstable();
    retained.dedup();
    retained.push(0);
    retained
  }

  fn simplify_open_indices(
    coordinates: &[WkbCoordinate],
    indices: &[usize],
    tolerance: f64,
  ) -> Vec<usize> {
    if indices.len() <= 2 {
      return indices.to_vec();
    }
    let mut keep = vec![false; indices.len()];
    keep[0] = true;
    keep[indices.len() - 1] = true;
    let mut ranges = vec![(0usize, indices.len() - 1)];
    let tolerance_squared = tolerance * tolerance;
    while let Some((start, end)) = ranges.pop() {
      let start_coordinate = coordinates[indices[start]];
      let end_coordinate = coordinates[indices[end]];
      let mut maximum = tolerance_squared;
      let mut selected = None;
      for candidate in start + 1..end {
        let distance = perpendicular_distance_squared(
          coordinates[indices[candidate]],
          start_coordinate,
          end_coordinate,
        );
        if distance > maximum {
          maximum = distance;
          selected = Some(candidate);
        }
      }
      if let Some(selected) = selected {
        keep[selected] = true;
        ranges.push((start, selected));
        ranges.push((selected, end));
      }
    }
    indices
      .iter()
      .zip(keep)
      .filter_map(|(index, keep)| keep.then_some(*index))
      .collect()
  }

  fn perpendicular_distance_squared(
    coordinate: WkbCoordinate,
    start: WkbCoordinate,
    end: WkbCoordinate,
  ) -> f64 {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length_squared = dx * dx + dy * dy;
    if length_squared == 0.0 {
      return squared_distance(coordinate, start);
    }
    let position = ((coordinate.x - start.x) * dx + (coordinate.y - start.y) * dy) / length_squared;
    let projected_x = start.x + position.clamp(0.0, 1.0) * dx;
    let projected_y = start.y + position.clamp(0.0, 1.0) * dy;
    let offset_x = coordinate.x - projected_x;
    let offset_y = coordinate.y - projected_y;
    offset_x * offset_x + offset_y * offset_y
  }

  fn squared_distance(left: WkbCoordinate, right: WkbCoordinate) -> f64 {
    let dx = left.x - right.x;
    let dy = left.y - right.y;
    dx * dx + dy * dy
  }

  fn same_xy(left: WkbCoordinate, right: WkbCoordinate) -> bool {
    left.x == right.x && left.y == right.y
  }

  fn coordinate_stride(has_z: bool, has_m: bool) -> usize {
    2 + usize::from(has_z) + usize::from(has_m)
  }

  fn quantize_axis(value: f64, encoding: &GeometryEncoding, axis: usize) -> Result<i64> {
    quantize(
      value,
      encoding.transform.scale[axis],
      encoding.transform.translate[axis],
    )
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

fn quantize_ordinate(value: Option<f64>, scale: f64, translate: f64) -> Result<i64> {
  match value {
    Some(value) if value.is_finite() => quantize(value, scale, translate),
    _ => Ok(0),
  }
}

fn is_collinear_delta(previous_dx: i64, previous_dy: i64, dx: i64, dy: i64) -> bool {
  previous_dx * dy == dx * previous_dy && (previous_dx * dx + previous_dy * dy) > 0
}

#[cfg(test)]
mod tests {
  use super::super::levels::QuantizationTransform;

  use super::*;

  fn coordinate(x: f64, y: f64) -> WkbCoordinate {
    WkbCoordinate {
      x,
      y,
      z: None,
      m: None,
    }
  }

  fn dimensional_coordinate(x: f64, y: f64, z: Option<f64>, m: Option<f64>) -> WkbCoordinate {
    WkbCoordinate { x, y, z, m }
  }

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
      &[
        coordinate(0.0, 0.0),
        coordinate(1.0, 1.0),
        coordinate(2.0, 2.0),
      ],
      &[3],
      &test_encoding(2),
      false,
      false,
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
      &[
        coordinate(0.1, 0.1),
        coordinate(0.2, 0.2),
        coordinate(0.9, 0.9),
        coordinate(1.0, 1.0),
      ],
      &[4],
      &test_encoding(2),
      false,
      false,
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
      &[coordinate(0.1, 0.1), coordinate(0.2, 0.2)],
      &[2],
      &test_encoding(2),
      false,
      false,
      &mut coords,
      &mut lengths,
    )
    .unwrap();
    assert_eq!(lengths, vec![1]);
    assert_eq!(coords, vec![0, 0]);
  }

  #[test]
  fn dimensional_dp_uses_xy_and_preserves_original_ordinates() {
    let mut coords = Vec::new();
    let mut lengths = Vec::new();
    encode_quantized_payload_into(
      &[
        dimensional_coordinate(0.0, 0.0, Some(10.0), Some(100.0)),
        dimensional_coordinate(1.0, 0.0, Some(999.0), Some(9999.0)),
        dimensional_coordinate(2.0, 0.0, Some(20.0), Some(200.0)),
      ],
      &[3],
      &test_encoding(2),
      true,
      true,
      &mut coords,
      &mut lengths,
    )
    .unwrap();

    assert_eq!(lengths, vec![2]);
    assert_eq!(coords, vec![0, 0, 10, 100, 2, 0, 20, 200]);
  }

  #[test]
  fn dimensional_pbf_delta_encodes_only_xy() {
    let mut coords = Vec::new();
    let mut lengths = Vec::new();
    encode_quantized_payload_into(
      &[
        dimensional_coordinate(10.0, 20.0, Some(100.0), Some(1000.0)),
        dimensional_coordinate(12.0, 23.0, Some(90.0), Some(900.0)),
      ],
      &[2],
      &test_encoding(2),
      true,
      true,
      &mut coords,
      &mut lengths,
    )
    .unwrap();

    assert_eq!(coords, vec![10, 20, 100, 1000, 2, 3, 90, 900]);
  }

  #[test]
  fn dimensional_pbf_encodes_missing_or_non_finite_ordinates_as_zero() {
    let mut coords = Vec::new();
    let mut lengths = Vec::new();
    encode_quantized_payload_into(
      &[
        dimensional_coordinate(0.0, 0.0, None, Some(f64::NAN)),
        dimensional_coordinate(2.0, 0.0, Some(f64::INFINITY), None),
      ],
      &[2],
      &test_encoding(2),
      true,
      true,
      &mut coords,
      &mut lengths,
    )
    .unwrap();

    assert_eq!(coords, vec![0, 0, 0, 0, 2, 0, 0, 0]);
  }
}

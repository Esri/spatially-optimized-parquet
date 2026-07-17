//! Converts floating-point geometry coordinates into the integer representation used by codecs.
//!
//! Callers supply the transform and simplification policy. Native Arrow preserves nullable z and m
//! components through [`ComponentValidity`], while Esri PBF encodes absent components as zero.

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use super::{Coord, Geometry};

/// Defines per-axis scale and translation values for integer coordinate quantization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct QuantizationTransform {
  /// Defines the distance represented by one integer unit for x, y, z, and m.
  pub(crate) scale: [f64; 4],
  /// Defines the coordinate origin for x, y, z, and m.
  pub(crate) translate: [f64; 4],
}

impl QuantizationTransform {
  /// Quantize one finite axis value using this transform.
  fn quantize(&self, value: f64, axis: usize) -> Result<i64> {
    let scale = self.scale[axis];
    if !scale.is_finite() || scale == 0.0 {
      bail!("quantization scale must be finite and non-zero");
    }
    let normalized = ((value - self.translate[axis]) / scale).round();
    if !normalized.is_finite() || normalized < i64::MIN as f64 || normalized > i64::MAX as f64 {
      bail!("quantized coordinate out of range")
    }
    Ok(normalized as i64)
  }
}

const Z_VALID: u8 = 1;
const M_VALID: u8 = 2;

#[derive(Default)]
/// Tracks nullable z and m components alongside integer coordinate values.
pub(crate) struct ComponentValidity {
  pub(crate) values: Vec<u8>,
}

impl ComponentValidity {
  /// Record z and m presence for one coordinate.
  pub(crate) fn push(&mut self, has_z: bool, has_m: bool) {
    self
      .values
      .push(u8::from(has_z) * Z_VALID | u8::from(has_m) * M_VALID);
  }

  /// Return whether the coordinate includes a z component.
  pub(crate) fn z_is_valid(&self, coordinate_index: usize) -> bool {
    self.values[coordinate_index] & Z_VALID != 0
  }

  /// Return whether the coordinate includes an m component.
  pub(crate) fn m_is_valid(&self, coordinate_index: usize) -> bool {
    self.values[coordinate_index] & M_VALID != 0
  }
}

#[derive(Default)]
/// Represents one simplified geometry as absolute quantized coordinates before codec encoding.
pub(crate) struct QuantizedGeometry {
  pub(crate) coordinates: Vec<i64>,
  pub(crate) lengths: Vec<u32>,
  pub(crate) validity: ComponentValidity,
  pub(crate) has_z: bool,
  pub(crate) has_m: bool,
}

#[derive(Debug, Clone, PartialEq)]
/// Configures coordinate snapping and simplification before geometry codecs consume a geometry.
pub(crate) struct QuantizationOptions {
  /// Provides the transform that maps source coordinates onto the integer grid.
  pub(crate) transform: QuantizationTransform,
  /// Defines the maximum perpendicular XY distance removed by dimensional simplification.
  pub(crate) tolerance: f64,
  /// Defines the minimum coordinate count required for a non-degenerate part.
  pub(crate) min_length: usize,
  /// Records whether output coordinates include the expected z component.
  pub(crate) has_z: bool,
  /// Records whether output coordinates include the expected m component.
  pub(crate) has_m: bool,
}

impl QuantizedGeometry {
  /// Quantize, simplify, and preserve valid coordinate components from geometry.
  pub(crate) fn quantize_from(
    &mut self,
    input: &Geometry,
    options: &QuantizationOptions,
  ) -> Result<()> {
    self.coordinates.clear();
    self.lengths.clear();
    self.validity.values.clear();

    let mut coordinate_offset = 0usize;
    let mut degenerated_coordinate = None::<Vec<i64>>;
    let mut degenerated_validity = None::<u8>;

    for &length in &input.lengths {
      let point_count = length as usize;
      if point_count == 0 {
        continue;
      }

      let part_start = self.coordinates.len();
      let validity_start = self.validity.values.len();
      let part = &input.coordinates[coordinate_offset..coordinate_offset + point_count];
      let output_length = if options.has_z || options.has_m {
        self.quantize_dimensional_part(part, options)?
      } else {
        self.quantize_xy_part(part, options)?
      };

      if output_length < options.min_length as u32 {
        degenerated_coordinate.get_or_insert_with(|| {
          self.coordinates[part_start..part_start + Self::coordinate_stride(options)].to_vec()
        });
        if options.has_z || options.has_m {
          degenerated_validity.get_or_insert(self.validity.values[validity_start]);
          self.validity.values.truncate(validity_start);
        }
        self.coordinates.truncate(part_start);
      } else {
        self.lengths.push(output_length);
      }
      coordinate_offset += point_count;
    }

    if self.lengths.is_empty()
      && let Some(coordinate) = degenerated_coordinate
    {
      self.coordinates.extend(coordinate);
      self.lengths.push(1);
      if let Some(validity) = degenerated_validity {
        self.validity.values.push(validity);
      }
    }

    self.has_z = options.has_z;
    self.has_m = options.has_m;
    Ok(())
  }

  fn quantize_xy_part(&mut self, part: &[Coord], options: &QuantizationOptions) -> Result<u32> {
    let first = part.first().expect("non-empty part");
    let mut previous_x = options.transform.quantize(first.x, 0)?;
    let mut previous_y = options.transform.quantize(first.y, 1)?;
    self.coordinates.extend([previous_x, previous_y]);
    let mut output_length = 1u32;
    let mut previous_dx = 0i64;
    let mut previous_dy = 0i64;

    for coordinate in &part[1..] {
      let x = options.transform.quantize(coordinate.x, 0)?;
      let y = options.transform.quantize(coordinate.y, 1)?;
      if x == previous_x && y == previous_y {
        continue;
      }
      let dx = x - previous_x;
      let dy = y - previous_y;
      if is_collinear_delta(previous_dx, previous_dy, dx, dy) {
        let coordinate_count = self.coordinates.len();
        self.coordinates[coordinate_count - 2] = x;
        self.coordinates[coordinate_count - 1] = y;
        previous_x = x;
        previous_y = y;
      } else {
        self.coordinates.extend([x, y]);
        previous_x = x;
        previous_y = y;
        previous_dx = dx;
        previous_dy = dy;
        output_length += 1;
      }
    }
    Ok(output_length)
  }

  fn quantize_dimensional_part(
    &mut self,
    part: &[Coord],
    options: &QuantizationOptions,
  ) -> Result<u32> {
    let retained = douglas_peucker_indices(part, options.tolerance);
    for input_index in retained.iter().copied() {
      let coordinate = part[input_index];
      self.coordinates.extend([
        options.transform.quantize(coordinate.x, 0)?,
        options.transform.quantize(coordinate.y, 1)?,
      ]);
      if options.has_z {
        self.coordinates.push(Self::quantize_optional_component(
          coordinate.z,
          &options.transform,
          2,
        )?);
      }
      if options.has_m {
        self.coordinates.push(Self::quantize_optional_component(
          coordinate.m,
          &options.transform,
          3,
        )?);
      }
      self.validity.push(
        options.has_z && coordinate.z.is_some_and(f64::is_finite),
        options.has_m && coordinate.m.is_some_and(f64::is_finite),
      );
    }
    Ok(u32::try_from(retained.len()).expect("geometry part length originates from u32"))
  }

  fn quantize_optional_component(
    value: Option<f64>,
    transform: &QuantizationTransform,
    axis: usize,
  ) -> Result<i64> {
    match value {
      Some(value) if value.is_finite() => transform.quantize(value, axis),
      _ => Ok(0),
    }
  }

  fn coordinate_stride(options: &QuantizationOptions) -> usize {
    2 + usize::from(options.has_z) + usize::from(options.has_m)
  }
}

fn is_collinear_delta(previous_dx: i64, previous_dy: i64, dx: i64, dy: i64) -> bool {
  previous_dx * dy == dx * previous_dy && (previous_dx * dx + previous_dy * dy) > 0
}

fn douglas_peucker_indices(coordinates: &[Coord], tolerance: f64) -> Vec<usize> {
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

fn simplify_open_indices(coordinates: &[Coord], indices: &[usize], tolerance: f64) -> Vec<usize> {
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

fn perpendicular_distance_squared(coordinate: Coord, start: Coord, end: Coord) -> f64 {
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

fn squared_distance(left: Coord, right: Coord) -> f64 {
  let dx = left.x - right.x;
  let dy = left.y - right.y;
  dx * dx + dy * dy
}

fn same_xy(left: Coord, right: Coord) -> bool {
  left.x == right.x && left.y == right.y
}

/// Delta encode x and y values independently within each geometry part.
pub(crate) fn encode_deltas_xy(coordinates: &mut [i64], lengths: &[u32], has_z: bool, has_m: bool) {
  let stride = 2 + usize::from(has_z) + usize::from(has_m);
  let mut coordinate_offset = 0usize;
  for &length in lengths {
    let mut previous_x = 0i64;
    let mut previous_y = 0i64;
    for point_index in 0..length as usize {
      let offset = coordinate_offset + point_index * stride;
      let x = coordinates[offset];
      let y = coordinates[offset + 1];
      if point_index > 0 {
        coordinates[offset] = x - previous_x;
        coordinates[offset + 1] = y - previous_y;
      }
      previous_x = x;
      previous_y = y;
    }
    coordinate_offset += length as usize * stride;
  }
}

#[cfg(test)]
mod tests {
  use super::{QuantizationOptions, QuantizationTransform, QuantizedGeometry};
  use crate::geometry::{Coord, Geometry, GeometryType};

  fn coordinate(x: f64, y: f64, z: Option<f64>, m: Option<f64>) -> Coord {
    Coord { x, y, z, m }
  }

  fn options(min_length: usize, has_z: bool, has_m: bool) -> QuantizationOptions {
    QuantizationOptions {
      transform: QuantizationTransform {
        scale: [1.0; 4],
        translate: [0.0; 4],
      },
      tolerance: 1.0,
      min_length,
      has_z,
      has_m,
    }
  }

  fn polyline(coordinates: Vec<Coord>) -> Geometry {
    Geometry::new(GeometryType::Polyline, coordinates, vec![2]).unwrap()
  }

  #[test]
  fn quantizes_coordinates_from_scale_and_translation() {
    let transform = QuantizationTransform {
      scale: [0.5, 1.0, 1.0, 1.0],
      translate: [10.0, 0.0, 0.0, 0.0],
    };

    assert_eq!(transform.quantize(11.0, 0).unwrap(), 2);
  }

  #[test]
  fn rejects_zero_scale() {
    let transform = QuantizationTransform {
      scale: [0.0; 4],
      translate: [0.0; 4],
    };

    assert!(transform.quantize(1.0, 0).is_err());
  }

  #[test]
  fn snaps_and_merges_collinear_xy_vertices() {
    let geometry = Geometry::new(
      GeometryType::Polyline,
      vec![
        coordinate(0.1, 0.1, None, None),
        coordinate(1.0, 1.0, None, None),
        coordinate(2.0, 2.0, None, None),
      ],
      vec![3],
    )
    .unwrap();
    let mut quantized = QuantizedGeometry::default();

    quantized
      .quantize_from(&geometry, &options(2, false, false))
      .unwrap();

    assert_eq!(quantized.lengths, [2]);
    assert_eq!(quantized.coordinates, [0, 0, 2, 2]);
  }

  #[test]
  fn drops_duplicate_snapped_vertices() {
    let geometry = Geometry::new(
      GeometryType::Polyline,
      vec![
        coordinate(0.1, 0.1, None, None),
        coordinate(0.2, 0.2, None, None),
        coordinate(0.9, 0.9, None, None),
        coordinate(1.0, 1.0, None, None),
      ],
      vec![4],
    )
    .unwrap();
    let mut quantized = QuantizedGeometry::default();

    quantized
      .quantize_from(&geometry, &options(2, false, false))
      .unwrap();

    assert_eq!(quantized.lengths, [2]);
    assert_eq!(quantized.coordinates, [0, 0, 1, 1]);
  }

  #[test]
  fn retains_original_dimensional_components_after_xy_simplification() {
    let geometry = Geometry::new(
      GeometryType::Polyline,
      vec![
        coordinate(0.0, 0.0, Some(10.0), Some(100.0)),
        coordinate(1.0, 0.0, Some(999.0), Some(9999.0)),
        coordinate(2.0, 0.0, Some(20.0), Some(200.0)),
      ],
      vec![3],
    )
    .unwrap();
    let mut quantized = QuantizedGeometry::default();

    quantized
      .quantize_from(&geometry, &options(2, true, true))
      .unwrap();

    assert_eq!(quantized.lengths, [2]);
    assert_eq!(quantized.coordinates, [0, 0, 10, 100, 2, 0, 20, 200]);
  }

  #[test]
  fn preserves_one_coordinate_for_degenerate_parts() {
    let geometry = polyline(vec![
      coordinate(0.1, 0.1, None, None),
      coordinate(0.2, 0.2, None, None),
    ]);
    let mut quantized = QuantizedGeometry::default();

    quantized
      .quantize_from(&geometry, &options(2, false, false))
      .unwrap();

    assert_eq!(quantized.lengths, [1]);
    assert_eq!(quantized.coordinates, [0, 0]);
  }

  #[test]
  fn records_degenerate_component_validity() {
    let geometry = polyline(vec![
      coordinate(0.0, 0.0, None, Some(1.0)),
      coordinate(0.1, 0.0, Some(2.0), None),
    ]);
    let mut quantized = QuantizedGeometry::default();

    quantized
      .quantize_from(&geometry, &options(3, true, true))
      .unwrap();

    assert_eq!(quantized.lengths, [1]);
    assert!(!quantized.validity.z_is_valid(0));
    assert!(quantized.validity.m_is_valid(0));
  }

  #[test]
  fn preserves_dimensional_validity_after_simplification() {
    let geometry = polyline(vec![
      coordinate(0.0, 0.0, Some(0.0), Some(f64::NAN)),
      coordinate(2.0, 0.0, None, Some(0.0)),
    ]);
    let mut quantized = QuantizedGeometry::default();

    quantized
      .quantize_from(&geometry, &options(2, true, true))
      .unwrap();

    assert_eq!(quantized.coordinates, [0, 0, 0, 0, 2, 0, 0, 0]);
    assert!(quantized.validity.z_is_valid(0));
    assert!(!quantized.validity.m_is_valid(0));
    assert!(!quantized.validity.z_is_valid(1));
    assert!(quantized.validity.m_is_valid(1));
  }

  #[test]
  fn encodes_missing_or_non_finite_components_as_zero() {
    let geometry = polyline(vec![
      coordinate(0.0, 0.0, None, Some(f64::NAN)),
      coordinate(2.0, 0.0, Some(f64::INFINITY), None),
    ]);
    let mut quantized = QuantizedGeometry::default();

    quantized
      .quantize_from(&geometry, &options(2, true, true))
      .unwrap();

    assert_eq!(quantized.coordinates, [0, 0, 0, 0, 2, 0, 0, 0]);
  }

  #[test]
  fn delta_encodes_only_xy_components() {
    let mut coordinates = vec![10, 20, 100, 1000, 12, 23, 90, 900];

    super::encode_deltas_xy(&mut coordinates, &[2], true, true);

    assert_eq!(coordinates, [10, 20, 100, 1000, 2, 3, 90, 900]);
  }
}

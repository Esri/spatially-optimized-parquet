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

//! Converts floating-point geometry coordinates into the integer representation used by codecs.
//!
//! Callers supply the transform and simplification policy. Missing Z/M components encode as zero.

use serde::{Deserialize, Serialize};

use super::{Coord, Geometry, GeometryError, GeometryType};

type Result<T> = std::result::Result<T, GeometryError>;

/// Defines per-axis scale and translation values for integer coordinate quantization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct QuantizationTransform {
  /// Defines the distance represented by one integer unit for x, y, z, and m.
  pub(crate) scale: [f64; 4],
  /// Defines the coordinate origin for x, y, z, and m.
  pub(crate) translate: [f64; 4],
}

impl QuantizationTransform {
  /// Quantize one axis value using this transform.
  fn quantize(&self, value: f64, axis: usize) -> i64 {
    ((value - self.translate[axis]) / self.scale[axis]).round() as i64
  }

  /// Convert one integer grid coordinate back to its transformed floating-point value.
  pub(crate) fn unquantize(&self, value: i64, axis: usize) -> f64 {
    value as f64 * self.scale[axis] + self.translate[axis]
  }
}

#[derive(Debug, Clone, PartialEq)]
/// Configures coordinate snapping and simplification before geometry codecs consume a geometry.
pub(crate) struct QuantizationOptions {
  /// Provides the transform that maps source coordinates onto the integer grid.
  pub(crate) transform: QuantizationTransform,
  /// Defines the maximum perpendicular XY distance removed by Z/M simplification.
  pub(crate) tolerance: f64,
  /// Defines the minimum coordinate count required for a non-degenerate part.
  pub(crate) min_length: usize,
  /// Records whether output coordinates include the expected z component.
  pub(crate) has_z: bool,
  /// Records whether output coordinates include the expected m component.
  pub(crate) has_m: bool,
}

/// Represents one simplified geometry as absolute quantized coordinates before codec encoding.
pub(crate) struct QuantizedGeometry {
  pub(crate) ty: GeometryType,
  pub(crate) coordinates: Vec<i64>,
  pub(crate) lengths: Vec<u32>,
  /// Preserve WKB polygon boundaries for writing non-PBF outputs.
  pub(crate) polygon_ring_counts: Vec<u32>,
  pub(crate) has_z: bool,
  pub(crate) has_m: bool,
}

impl Default for QuantizedGeometry {
  /// Build an empty geometry with reusable coordinate and part buffers.
  fn default() -> Self {
    Self {
      ty: GeometryType::Unknown,
      coordinates: Vec::new(),
      lengths: Vec::new(),
      polygon_ring_counts: Vec::new(),
      has_z: false,
      has_m: false,
    }
  }
}

/// Write quantized geometry from source coordinates into a reusable output buffer.
/// See spec/display-optimization.md, Generating Multiscale Columns and Multiscale Requirements.
pub(crate) fn quantize_into(
  input: &Geometry,
  options: &QuantizationOptions,
  output: &mut QuantizedGeometry,
) -> Result<()> {
  output.coordinates.clear();
  output.lengths.clear();
  output.polygon_ring_counts.clear();
  output.ty = input.ty;
  output.has_z = options.has_z;
  output.has_m = options.has_m;

  match input.ty {
    GeometryType::Point => quantize_point(output, input, options),
    GeometryType::MultiPoint => quantize_multipoint(output, input, options),
    GeometryType::LineString | GeometryType::MultiLineString => {
      quantize_parts(output, input, options)
    }
    GeometryType::Polygon | GeometryType::MultiPolygon => quantize_parts(output, input, options),
    _ => {
      return Err(GeometryError::UnsupportedGeometryType(input.ty));
    }
  }

  // Preserve a quantized first source vertex when all parts collapse so visualization has a coordinate to render.
  if output.lengths.is_empty()
    && let Some(vertex) = input.coordinates.first()
  {
    quantize_xyzm_to_buffer(&mut output.coordinates, vertex, options);
    output.lengths.push(1);
    if matches!(input.ty, GeometryType::Polygon | GeometryType::MultiPolygon) {
      output.polygon_ring_counts.push(1);
    }
  }

  Ok(())
}

/// Quantize an XY position from source coordinates using the supplied transform.
fn quantize_xy(x: f64, y: f64, transform: &QuantizationTransform) -> (i64, i64) {
  let x = transform.quantize(x, 0);
  let y = transform.quantize(y, 1);

  (x, y)
}

/// Write one quantized coordinate from a source vertex with the requested Z/M components.
fn quantize_xyzm_to_buffer(coords: &mut Vec<i64>, vertex: &Coord, options: &QuantizationOptions) {
  let transform = &options.transform;
  let (x, y) = quantize_xy(vertex.x, vertex.y, transform);
  coords.push(x);
  coords.push(y);
  if options.has_z {
    coords.push(vertex.z.map_or(0, |value| transform.quantize(value, 2)));
  }
  if options.has_m {
    coords.push(vertex.m.map_or(0, |value| transform.quantize(value, 3)));
  }
}

/// Quantize one point from its source geometry without part simplification.
fn quantize_point(
  output: &mut QuantizedGeometry,
  geometry: &Geometry,
  options: &QuantizationOptions,
) {
  let Some(vertex) = geometry.coordinates.first() else {
    return;
  };
  quantize_xyzm_to_buffer(&mut output.coordinates, vertex, options);
  output.lengths.push(1);
}

/// Quantize multipoint vertices from source geometry without merging their positions.
fn quantize_multipoint(
  output: &mut QuantizedGeometry,
  geometry: &Geometry,
  options: &QuantizationOptions,
) {
  for vertex in &geometry.coordinates {
    quantize_xyzm_to_buffer(&mut output.coordinates, vertex, options);
  }

  output.lengths.extend_from_slice(&geometry.lengths);
}

/// Quantize line or polygon parts from source geometry, discarding parts below the minimum length.
/// See spec/display-optimization.md, XY Quantization.
fn quantize_parts(
  output: &mut QuantizedGeometry,
  geometry: &Geometry,
  options: &QuantizationOptions,
) {
  let coords = &mut output.coordinates;
  let lengths = &mut output.lengths;
  let transform = &options.transform;
  let mut offset = 0;

  for &length in &geometry.lengths {
    let part = &geometry.coordinates[offset..offset + length as usize];
    offset += length as usize;
    let ring_start_coord_index = coords.len();
    let out_length = if options.has_z || options.has_m {
      quantize_part_xyzm(coords, part, options)
    } else {
      quantize_part_xy(coords, part, transform)
    };

    if (out_length as usize) < options.min_length {
      coords.truncate(ring_start_coord_index);
      lengths.push(0);
    } else {
      lengths.push(out_length);
    }
  }

  // Keep discarded part slots until polygon ring counts have been rebuilt.
  let mut offset = 0;
  for &ring_count in &geometry.polygon_ring_counts {
    let part_end = offset + ring_count as usize;
    let retained_count = lengths[offset..part_end]
      .iter()
      .filter(|&&length| length > 0)
      .count();
    if retained_count > 0 {
      output.polygon_ring_counts.push(retained_count as u32);
    }
    offset = part_end;
  }
  lengths.retain(|&length| length > 0);
}

/// Quantize an XY part by snapping first, then merging grid duplicates and collinear runs.
fn quantize_part_xy(
  coords: &mut Vec<i64>,
  part: &[Coord],
  transform: &QuantizationTransform,
) -> u32 {
  let Some((vertex, vertices)) = part.split_first() else {
    return 0;
  };
  let (mut prev_x, mut prev_y) = quantize_xy(vertex.x, vertex.y, transform);
  coords.push(prev_x);
  coords.push(prev_y);
  let mut out_length = 1;
  let mut prev_dx = 0;
  let mut prev_dy = 0;

  for vertex in vertices {
    let (x, y) = quantize_xy(vertex.x, vertex.y, transform);
    if x != prev_x || y != prev_y {
      let dx = x - prev_x;
      let dy = y - prev_y;
      if is_collinear_delta(prev_dx, prev_dy, dx, dy) {
        let len = coords.len();
        coords[len - 2] = x;
        coords[len - 1] = y;
      } else {
        coords.push(x);
        coords.push(y);
        prev_dx = dx;
        prev_dy = dy;
        out_length += 1;
      }
      prev_x = x;
      prev_y = y;
    }
  }

  out_length
}

/// Quantize an XYZM part with XY-only Douglas-Peucker before snapping to apply the source-space tolerance.
fn quantize_part_xyzm(coords: &mut Vec<i64>, part: &[Coord], options: &QuantizationOptions) -> u32 {
  let part_start = coords.len();
  let stride = 2 + usize::from(options.has_z) + usize::from(options.has_m);
  let retained = douglas_peucker_indices(part, options.tolerance);

  for index in retained {
    let vertex_start = coords.len();
    quantize_xyzm_to_buffer(coords, &part[index], options);

    if vertex_start > part_start
      && coords[vertex_start - stride..vertex_start] == coords[vertex_start..]
    {
      coords.truncate(vertex_start);
    }
  }

  ((coords.len() - part_start) / stride) as u32
}

/// Resolve whether consecutive XY deltas follow the same straight-line direction.
fn is_collinear_delta(previous_dx: i64, previous_dy: i64, dx: i64, dy: i64) -> bool {
  let previous_dx = i128::from(previous_dx);
  let previous_dy = i128::from(previous_dy);
  let dx = i128::from(dx);
  let dy = i128::from(dy);

  previous_dx * dy == dx * previous_dy && previous_dx * dx + previous_dy * dy > 0
}

/// Build retained source indices by replacing path sections with straight segments within XY tolerance.
/// Use iterative [Douglas-Peucker](https://en.wikipedia.org/wiki/Ramer%E2%80%93Douglas%E2%80%93Peucker_algorithm) without interpolating vertex values.
fn douglas_peucker_indices(coordinates: &[Coord], tolerance: f64) -> Vec<usize> {
  if coordinates.len() <= 2 {
    return (0..coordinates.len()).collect();
  }

  let last_index = coordinates.len() - 1;
  let closed = same_xy(coordinates[0], coordinates[last_index]);
  if closed && coordinates.len() <= 4 {
    // Already a triangle, can't simplify further.
    return (0..coordinates.len()).collect();
  }

  // First initialize the stack with source sections to simplify, each including its start and end.
  let mut stack = Vec::new();
  if closed {
    // Split a closed ring into three sections so coincident endpoints cannot erase its interior.
    let (first_split, second_split) = ring_split_indices(coordinates);
    stack.push((second_split, last_index));
    stack.push((first_split, second_split));
    stack.push((0, first_split));
  } else {
    // Start an open path with one section so its endpoints bound the first simplification attempt.
    stack.push((0, last_index));
  }

  let mut retained_indices = vec![0];
  let tolerance_squared = tolerance * tolerance;

  // Next test whether one straight segment can replace the section within tolerance:
  //
  //   start ---- interior vertices ---- end
  //                 |
  //                 +-- all within tolerance: keep only start and end
  //                 +-- farthest exceeds tolerance: split there and test both sections
  while let Some((start_index, end_index)) = stack.pop() {
    if let Some((farthest_index, distance_squared)) =
      farthest_vertex(coordinates, start_index, end_index)
      && distance_squared > tolerance_squared
    {
      // Preserve the bend as a shared endpoint, processing the left section first to keep order.
      stack.push((farthest_index, end_index));
      stack.push((start_index, farthest_index));
    } else {
      // Skip the interior: joining retained endpoints removes detail within the XY tolerance.
      retained_indices.push(end_index);
    }
  }

  retained_indices
}

/// Resolve two interior split indices for a closed ring, retaining them along with vertex 0.
/// Preserve source order without guaranteeing nonzero area or valid topology.
fn ring_split_indices(coordinates: &[Coord]) -> (usize, usize) {
  let last_index = coordinates.len() - 1;
  // Keep the vertex farthest from vertex 0, whose XY also closes the ring.
  let (farthest_index, _) =
    farthest_vertex(coordinates, 0, last_index).expect("ring has interior vertices");
  // Keep the largest remaining deviation so the initial split retains another bend.
  let before = farthest_vertex(coordinates, 0, farthest_index);
  let after = farthest_vertex(coordinates, farthest_index, last_index);
  let other_index = match (before, after) {
    // Keep the later candidate only when its deviation exceeds the earlier candidate's.
    (Some(before), Some(after)) if after.1 > before.1 => after.0,
    // Keep the earlier candidate when it wins, ties, or has no competitor.
    (Some(before), _) => before.0,
    // Keep the later candidate when the earlier section has no interior vertex.
    (None, Some(after)) => after.0,
    (None, None) => unreachable!("ring has another interior vertex"),
  };

  (
    farthest_index.min(other_index),
    farthest_index.max(other_index),
  )
}

/// Resolve the interior vertex farthest from the section's endpoint segment and its squared XY distance.
/// Return None when the section has no interior vertices.
fn farthest_vertex(
  coordinates: &[Coord],
  start_index: usize,
  end_index: usize,
) -> Option<(usize, f64)> {
  let mut farthest = None;

  for index in start_index + 1..end_index {
    let distance_squared = point_segment_distance_squared(
      coordinates[index],
      coordinates[start_index],
      coordinates[end_index],
    );
    if farthest.is_none_or(|(_, maximum_squared)| distance_squared > maximum_squared) {
      farthest = Some((index, distance_squared));
    }
  }

  farthest
}

/// Resolve the squared XY distance from a coordinate to a bounded segment.
fn point_segment_distance_squared(coordinate: Coord, start: Coord, end: Coord) -> f64 {
  let dx = end.x - start.x;
  let dy = end.y - start.y;
  let length_squared = dx * dx + dy * dy;
  if length_squared == 0.0 {
    return distance_squared(coordinate, start);
  }
  let position = ((coordinate.x - start.x) * dx + (coordinate.y - start.y) * dy) / length_squared;
  let projected_x = start.x + position.clamp(0.0, 1.0) * dx;
  let projected_y = start.y + position.clamp(0.0, 1.0) * dy;
  let offset_x = coordinate.x - projected_x;
  let offset_y = coordinate.y - projected_y;
  offset_x * offset_x + offset_y * offset_y
}

/// Resolve the squared XY distance from two coordinates.
fn distance_squared(left: Coord, right: Coord) -> f64 {
  let dx = left.x - right.x;
  let dy = left.y - right.y;
  dx * dx + dy * dy
}

/// Resolve whether two coordinates share the same XY position.
fn same_xy(left: Coord, right: Coord) -> bool {
  left.x == right.x && left.y == right.y
}

#[cfg(test)]
mod tests {
  use super::{
    QuantizationOptions, QuantizationTransform, QuantizedGeometry, douglas_peucker_indices,
    is_collinear_delta, point_segment_distance_squared, quantize_into,
  };
  use crate::geometry::{Coord, Geometry, GeometryType};

  mod dispatch {
    use super::{Geometry, GeometryType, QuantizedGeometry, options, quantize_into};
    use crate::geometry::GeometryError;

    #[test]
    fn rejects_unsupported_geometry_types() {
      for ty in [GeometryType::GeometryCollection, GeometryType::Unknown] {
        let geometry = Geometry {
          ty,
          coordinates: Vec::new(),
          lengths: Vec::new(),
          polygon_ring_counts: Vec::new(),
        };
        let mut quantized = QuantizedGeometry::default();

        let error =
          quantize_into(&geometry, &options(2, false, false), &mut quantized).unwrap_err();

        assert!(matches!(error, GeometryError::UnsupportedGeometryType(value) if value == ty));
      }
    }
  }

  mod transform {
    use super::{QuantizationTransform, options};

    #[test]
    fn quantizes_with_scale_and_translation() {
      let transform = QuantizationTransform {
        scale: [0.5, 1.0, 1.0, 1.0],
        translate: [10.0, 0.0, 0.0, 0.0],
      };

      assert_eq!(transform.quantize(11.0, 0), 2);
    }

    #[test]
    fn rounds_half_grid_values_away_from_zero() {
      let transform = options(1, false, false).transform;

      for (value, expected) in [(-1.5, -2), (-0.5, -1), (0.5, 1), (1.5, 2)] {
        assert_eq!(transform.quantize(value, 0), expected);
      }
    }

    #[test]
    fn applies_each_axis_transform_independently() {
      let transform = QuantizationTransform {
        scale: [0.5, -2.0, 4.0, 0.25],
        translate: [10.0, 20.0, -8.0, 100.0],
      };

      for (axis, value, expected) in [(0, 11.0, 2), (1, 26.0, -3), (2, 4.0, 3), (3, 99.0, -4)] {
        assert_eq!(transform.quantize(value, axis), expected);
        assert_eq!(transform.unquantize(expected, axis), value);
      }
    }

    #[test]
    fn bounds_round_trip_error_by_half_the_grid_spacing() {
      let transform = QuantizationTransform {
        scale: [0.5, -2.0, 4.0, 0.25],
        translate: [10.0, 20.0, -8.0, 100.0],
      };

      for axis in 0..4 {
        for value in [-100.25, -0.25, 0.0, 0.25, 100.25] {
          let quantized = transform.quantize(value, axis);
          let restored = transform.unquantize(quantized, axis);

          assert!((restored - value).abs() <= transform.scale[axis].abs() / 2.0);
        }
      }
    }
  }

  mod point {
    use super::{Geometry, GeometryType, QuantizedGeometry, coordinate, options, quantize_into};

    #[test]
    fn quantizes_each_layout() {
      for (has_z, has_m, expected) in [
        (false, false, vec![1, 2]),
        (true, false, vec![1, 2, 3]),
        (false, true, vec![1, 2, 4]),
        (true, true, vec![1, 2, 3, 4]),
      ] {
        let geometry = Geometry::new(
          GeometryType::Point,
          vec![coordinate(1.1, 2.1, Some(3.1), Some(4.1))],
          vec![1],
          Vec::new(),
        )
        .unwrap();
        let mut quantized = QuantizedGeometry::default();

        quantize_into(&geometry, &options(1, has_z, has_m), &mut quantized).unwrap();

        assert_eq!(quantized.coordinates, expected);
        assert_eq!(quantized.lengths, [1]);
        assert_eq!(quantized.ty, GeometryType::Point);
      }
    }
  }

  mod multipoint {
    use super::{Geometry, GeometryType, QuantizedGeometry, coordinate, options, quantize_into};

    #[test]
    fn preserves_collinear_vertices() {
      for (has_z, has_m) in [(false, false), (true, false), (false, true), (true, true)] {
        let geometry = Geometry::new(
          GeometryType::MultiPoint,
          vec![
            coordinate(0.0, 0.0, Some(1.0), Some(10.0)),
            coordinate(1.0, 0.0, Some(2.0), Some(20.0)),
            coordinate(2.0, 0.0, Some(3.0), Some(30.0)),
          ],
          vec![3],
          Vec::new(),
        )
        .unwrap();
        let mut quantized = QuantizedGeometry::default();

        quantize_into(&geometry, &options(1, has_z, has_m), &mut quantized).unwrap();

        assert_eq!(quantized.ty, GeometryType::MultiPoint);
        assert_eq!(quantized.lengths, [3], "has_z: {has_z}, has_m: {has_m}");
      }
    }

    #[test]
    fn preserves_vertices_that_snap_to_the_same_position() {
      for (has_z, has_m) in [(false, false), (true, false), (false, true), (true, true)] {
        let geometry = Geometry::new(
          GeometryType::MultiPoint,
          vec![
            coordinate(0.1, 0.1, Some(1.0), Some(2.0)),
            coordinate(0.2, 0.2, Some(1.0), Some(2.0)),
            coordinate(0.3, 0.3, Some(1.0), Some(2.0)),
          ],
          vec![3],
          Vec::new(),
        )
        .unwrap();
        let mut quantized = QuantizedGeometry::default();

        quantize_into(&geometry, &options(1, has_z, has_m), &mut quantized).unwrap();

        assert_eq!(quantized.lengths, [3]);
        let stride = 2 + usize::from(has_z) + usize::from(has_m);
        assert_eq!(quantized.coordinates.len(), 3 * stride);
        assert!(
          quantized
            .coordinates
            .chunks_exact(stride)
            .all(|vertex| vertex[..2] == [0, 0])
        );
      }
    }
  }

  mod xy {
    use super::{
      Geometry, GeometryType, QuantizedGeometry, coordinate, options, polyline, quantize_into, xy,
    };

    #[test]
    fn snaps_and_merges_collinear_vertices() {
      let geometry = Geometry::new(
        GeometryType::LineString,
        vec![
          coordinate(0.1, 0.1, None, None),
          coordinate(1.0, 1.0, None, None),
          coordinate(2.0, 2.0, None, None),
        ],
        vec![3],
        Vec::new(),
      )
      .unwrap();
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(2, false, false), &mut quantized).unwrap();

      assert_eq!(quantized.lengths, [2]);
      assert_eq!(quantized.coordinates, [0, 0, 2, 2]);
    }

    #[test]
    fn removes_duplicate_snapped_vertices() {
      let geometry = Geometry::new(
        GeometryType::LineString,
        vec![
          coordinate(0.1, 0.1, None, None),
          coordinate(0.2, 0.2, None, None),
          coordinate(0.9, 0.9, None, None),
          coordinate(1.0, 1.0, None, None),
        ],
        vec![4],
        Vec::new(),
      )
      .unwrap();
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(2, false, false), &mut quantized).unwrap();

      assert_eq!(quantized.lengths, [2]);
      assert_eq!(quantized.coordinates, [0, 0, 1, 1]);
    }

    #[test]
    fn preserves_direction_reversals() {
      let geometry = polyline(xy(&[(0.0, 0.0), (2.0, 0.0), (1.0, 0.0)]));
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(2, false, false), &mut quantized).unwrap();

      assert_eq!(quantized.lengths, [3]);
      assert_eq!(quantized.coordinates, [0, 0, 2, 0, 1, 0]);
    }
  }

  mod xyz {
    use super::{QuantizedGeometry, coordinate, options, polyline, quantize_into};

    #[test]
    fn quantizes_z_with_its_own_scale_and_origin() {
      let geometry = polyline(vec![
        coordinate(0.0, 0.0, Some(12.0), None),
        coordinate(2.0, 0.0, Some(18.0), None),
      ]);
      let mut options = options(2, true, false);
      options.transform.scale[2] = 2.0;
      options.transform.translate[2] = 10.0;
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options, &mut quantized).unwrap();

      assert_eq!(quantized.coordinates, [0, 0, 1, 2, 0, 4]);
      assert!(quantized.has_z);
      assert!(!quantized.has_m);
    }

    #[test]
    fn encodes_missing_z_as_zero() {
      let geometry = polyline(vec![
        coordinate(0.0, 0.0, None, None),
        coordinate(2.0, 0.0, Some(0.0), None),
      ]);
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(2, true, false), &mut quantized).unwrap();

      assert_eq!(quantized.coordinates, [0, 0, 0, 2, 0, 0]);
    }

    #[test]
    fn preserves_z_changes_at_coincident_snapped_endpoints() {
      let geometry = polyline(vec![
        coordinate(0.1, 0.1, Some(10.0), None),
        coordinate(0.2, 0.2, Some(20.0), None),
      ]);
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(2, true, false), &mut quantized).unwrap();

      assert_eq!(quantized.lengths, [2]);
      assert_eq!(quantized.coordinates, [0, 0, 10, 0, 0, 20]);
    }

    #[test]
    fn preserves_the_closing_z_of_a_closed_polyline() {
      let geometry = polyline(vec![
        coordinate(0.0, 0.0, Some(10.0), None),
        coordinate(4.0, 0.0, Some(20.0), None),
        coordinate(4.0, 4.0, Some(30.0), None),
        coordinate(0.0, 4.0, Some(40.0), None),
        coordinate(0.0, 0.0, Some(50.0), None),
      ]);
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(2, true, false), &mut quantized).unwrap();

      assert_eq!(quantized.lengths, [5]);
      assert_eq!(&quantized.coordinates[12..], [0, 0, 50]);
    }
  }

  mod xym {
    use super::{QuantizedGeometry, coordinate, options, polyline, quantize_into};

    #[test]
    fn quantizes_m_without_a_z_component() {
      let geometry = polyline(vec![
        coordinate(0.0, 0.0, None, Some(102.0)),
        coordinate(2.0, 0.0, None, Some(108.0)),
      ]);
      let mut options = options(2, false, true);
      options.transform.scale[3] = 2.0;
      options.transform.translate[3] = 100.0;
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options, &mut quantized).unwrap();

      assert_eq!(quantized.coordinates, [0, 0, 1, 2, 0, 4]);
      assert!(!quantized.has_z);
      assert!(quantized.has_m);
    }

    #[test]
    fn encodes_missing_m_as_zero() {
      let geometry = polyline(vec![
        coordinate(0.0, 0.0, None, None),
        coordinate(2.0, 0.0, None, Some(0.0)),
      ]);
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(2, false, true), &mut quantized).unwrap();

      assert_eq!(quantized.coordinates, [0, 0, 0, 2, 0, 0]);
    }

    #[test]
    fn retains_original_m_after_xy_simplification() {
      let geometry = polyline(vec![
        coordinate(0.0, 0.0, None, Some(10.0)),
        coordinate(1.0, 0.0, None, Some(999.0)),
        coordinate(2.0, 0.0, None, Some(20.0)),
      ]);
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(2, false, true), &mut quantized).unwrap();

      assert_eq!(quantized.lengths, [2]);
      assert_eq!(quantized.coordinates, [0, 0, 10, 2, 0, 20]);
    }

    #[test]
    fn preserves_the_closing_m_of_a_closed_polyline() {
      let geometry = polyline(vec![
        coordinate(0.0, 0.0, None, Some(10.0)),
        coordinate(4.0, 0.0, None, Some(20.0)),
        coordinate(4.0, 4.0, None, Some(30.0)),
        coordinate(0.0, 4.0, None, Some(40.0)),
        coordinate(0.0, 0.0, None, Some(50.0)),
      ]);
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(2, false, true), &mut quantized).unwrap();

      assert_eq!(quantized.lengths, [5]);
      assert_eq!(&quantized.coordinates[12..], [0, 0, 50]);
    }
  }

  mod xyzm {
    use super::{
      Geometry, GeometryType, QuantizedGeometry, coordinate, options, polyline, quantize_into,
    };

    #[test]
    fn quantizes_z_and_m_with_independent_transforms() {
      let geometry = polyline(vec![
        coordinate(0.0, 0.0, Some(12.0), Some(102.0)),
        coordinate(2.0, 0.0, Some(18.0), Some(108.0)),
      ]);
      let mut options = options(2, true, true);
      options.transform.scale = [1.0, 1.0, 2.0, 0.5];
      options.transform.translate = [0.0, 0.0, 10.0, 100.0];
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options, &mut quantized).unwrap();

      assert_eq!(quantized.coordinates, [0, 0, 1, 4, 2, 0, 4, 16]);
    }

    #[test]
    fn retains_original_zm_after_xy_simplification() {
      let geometry = Geometry::new(
        GeometryType::LineString,
        vec![
          coordinate(0.0, 0.0, Some(10.0), Some(100.0)),
          coordinate(1.0, 0.0, Some(999.0), Some(9999.0)),
          coordinate(2.0, 0.0, Some(20.0), Some(200.0)),
        ],
        vec![3],
        Vec::new(),
      )
      .unwrap();
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(2, true, true), &mut quantized).unwrap();

      assert_eq!(quantized.lengths, [2]);
      assert_eq!(quantized.coordinates, [0, 0, 10, 100, 2, 0, 20, 200]);
    }

    #[test]
    fn encodes_missing_z_and_m_as_zero() {
      let geometry = polyline(vec![
        coordinate(0.0, 0.0, None, None),
        coordinate(2.0, 0.0, None, None),
      ]);
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(2, true, true), &mut quantized).unwrap();

      assert_eq!(quantized.coordinates, [0, 0, 0, 0, 2, 0, 0, 0]);
    }

    #[test]
    fn encodes_missing_and_present_zero_identically() {
      let geometry = polyline(vec![
        coordinate(0.0, 0.0, Some(0.0), None),
        coordinate(2.0, 0.0, None, Some(0.0)),
      ]);
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(2, true, true), &mut quantized).unwrap();

      assert_eq!(quantized.coordinates, [0, 0, 0, 0, 2, 0, 0, 0]);
    }

    #[test]
    fn encodes_mixed_missing_components_as_zero() {
      let geometry = polyline(vec![
        coordinate(0.0, 0.0, None, None),
        coordinate(1.0, 4.0, Some(1.0), None),
        coordinate(2.0, -4.0, None, Some(2.0)),
        coordinate(3.0, 0.0, Some(3.0), Some(4.0)),
      ]);
      let mut options = options(2, true, true);
      options.tolerance = 0.0;
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options, &mut quantized).unwrap();

      assert_eq!(quantized.lengths, [4]);
      assert_eq!(
        quantized.coordinates,
        [0, 0, 0, 0, 1, 4, 1, 0, 2, -4, 0, 2, 3, 0, 3, 4]
      );
    }

    #[test]
    fn preserves_zm_changes_at_coincident_snapped_endpoints() {
      let geometry = polyline(vec![
        coordinate(0.1, 0.1, Some(10.0), Some(100.0)),
        coordinate(0.2, 0.2, Some(20.0), Some(200.0)),
      ]);
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(2, true, true), &mut quantized).unwrap();

      assert_eq!(quantized.lengths, [2]);
      assert_eq!(quantized.coordinates, [0, 0, 10, 100, 0, 0, 20, 200]);
    }

    #[test]
    fn merges_identical_snapped_vertices() {
      let geometry = polyline(vec![
        coordinate(0.1, 0.1, Some(10.0), Some(100.0)),
        coordinate(0.2, 0.2, Some(10.0), Some(100.0)),
      ]);
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(2, true, true), &mut quantized).unwrap();

      assert_eq!(quantized.lengths, [1]);
      assert_eq!(quantized.coordinates, [0, 0, 10, 100]);
    }

    #[test]
    fn merges_missing_components_with_zero_neighbors() {
      let geometry = polyline(vec![
        coordinate(0.1, 0.1, None, Some(0.0)),
        coordinate(0.2, 0.2, Some(0.0), None),
      ]);
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(2, true, true), &mut quantized).unwrap();

      assert_eq!(quantized.lengths, [1]);
      assert_eq!(quantized.coordinates, [0, 0, 0, 0]);
    }

    #[test]
    fn preserves_closing_m_with_missing_z_as_zero() {
      let geometry = polyline(vec![
        coordinate(0.0, 0.0, Some(10.0), None),
        coordinate(4.0, 0.0, Some(20.0), Some(200.0)),
        coordinate(4.0, 4.0, Some(30.0), Some(300.0)),
        coordinate(0.0, 4.0, Some(40.0), Some(400.0)),
        coordinate(0.0, 0.0, None, Some(500.0)),
      ]);
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(2, true, true), &mut quantized).unwrap();

      assert_eq!(&quantized.coordinates[16..], [0, 0, 0, 500]);
    }
  }

  mod part {
    use super::{
      Geometry, GeometryType, QuantizedGeometry, coordinate, options, polyline, quantize_into, xy,
    };

    #[test]
    fn retains_shared_endpoints_in_separate_parts() {
      let geometry = Geometry::new(
        GeometryType::MultiLineString,
        xy(&[(0.0, 0.0), (1.0, 0.0), (1.0, 0.0), (2.0, 0.0)]),
        vec![2, 2],
        Vec::new(),
      )
      .unwrap();
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(2, false, false), &mut quantized).unwrap();

      assert_eq!(quantized.lengths, [2, 2]);
      assert_eq!(quantized.coordinates, [0, 0, 1, 0, 1, 0, 2, 0]);
    }

    #[test]
    fn simplifies_xyzm_parts_independently() {
      let geometry = Geometry::new(
        GeometryType::MultiLineString,
        vec![
          coordinate(0.0, 0.0, Some(1.0), Some(10.0)),
          coordinate(1.0, 0.0, Some(2.0), Some(20.0)),
          coordinate(2.0, 0.0, Some(3.0), Some(30.0)),
          coordinate(2.0, 0.0, Some(4.0), Some(40.0)),
          coordinate(3.0, 0.0, Some(5.0), Some(50.0)),
          coordinate(4.0, 0.0, Some(6.0), Some(60.0)),
        ],
        vec![3, 3],
        Vec::new(),
      )
      .unwrap();
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(2, true, true), &mut quantized).unwrap();

      assert_eq!(quantized.lengths, [2, 2]);
      assert_eq!(
        quantized.coordinates,
        [0, 0, 1, 10, 2, 0, 3, 30, 2, 0, 4, 40, 4, 0, 6, 60]
      );
    }

    #[test]
    fn preserves_multipolygon_ring_grouping() {
      let geometry = Geometry::new(
        GeometryType::MultiPolygon,
        vec![
          coordinate(0.0, 0.0, None, None),
          coordinate(4.0, 0.0, None, None),
          coordinate(4.0, 4.0, None, None),
          coordinate(0.0, 4.0, None, None),
          coordinate(0.0, 0.0, None, None),
          coordinate(10.0, 10.0, None, None),
          coordinate(14.0, 10.0, None, None),
          coordinate(14.0, 14.0, None, None),
          coordinate(10.0, 14.0, None, None),
          coordinate(10.0, 10.0, None, None),
        ],
        vec![5, 5],
        vec![1, 1],
      )
      .unwrap();
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(3, false, false), &mut quantized).unwrap();

      assert_eq!(quantized.ty, GeometryType::MultiPolygon);
      assert_eq!(quantized.lengths, [5, 5]);
      assert_eq!(quantized.polygon_ring_counts, [1, 1]);
    }

    #[test]
    fn preserves_polygon_grouping_after_discarding_collapsed_rings() {
      let geometry = Geometry::new(
        GeometryType::MultiPolygon,
        xy(&[
          (0.1, 0.1),
          (0.2, 0.1),
          (0.2, 0.2),
          (0.1, 0.1),
          (10.0, 10.0),
          (14.0, 10.0),
          (10.0, 14.0),
          (10.0, 10.0),
          (11.0, 11.0),
          (12.0, 11.0),
          (11.0, 12.0),
          (11.0, 11.0),
        ]),
        vec![4, 4, 4],
        vec![1, 2],
      )
      .unwrap();
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(3, false, false), &mut quantized).unwrap();

      assert_eq!(quantized.ty, GeometryType::MultiPolygon);
      assert_eq!(quantized.lengths, [4, 4]);
      assert_eq!(quantized.polygon_ring_counts, [2]);
      assert_eq!(&quantized.coordinates[..2], [10, 10]);
    }

    #[test]
    fn preserves_polygon_grouping_after_empty_rings() {
      let geometry = Geometry::new(
        GeometryType::MultiPolygon,
        xy(&[(0.0, 0.0), (4.0, 0.0), (0.0, 4.0), (0.0, 0.0)]),
        vec![0, 4],
        vec![1, 1],
      )
      .unwrap();
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(3, false, false), &mut quantized).unwrap();

      assert_eq!(quantized.lengths, [4]);
      assert_eq!(quantized.polygon_ring_counts, [1]);
    }

    #[test]
    fn discards_collapsed_parts_when_another_part_survives() {
      let geometry = Geometry::new(
        GeometryType::MultiLineString,
        xy(&[(0.1, 0.1), (0.2, 0.2), (10.0, 10.0), (12.0, 10.0)]),
        vec![2, 2],
        Vec::new(),
      )
      .unwrap();
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(2, false, false), &mut quantized).unwrap();

      assert_eq!(quantized.lengths, [2]);
      assert_eq!(quantized.coordinates, [10, 10, 12, 10]);
    }

    #[test]
    fn discards_xyzm_coordinates_from_collapsed_parts() {
      let geometry = Geometry::new(
        GeometryType::MultiLineString,
        vec![
          coordinate(0.0, 0.0, None, None),
          coordinate(10.0, 10.0, Some(1.0), None),
          coordinate(12.0, 10.0, None, Some(2.0)),
        ],
        vec![1, 2],
        Vec::new(),
      )
      .unwrap();
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(2, true, true), &mut quantized).unwrap();

      assert_eq!(quantized.lengths, [2]);
      assert_eq!(quantized.coordinates, [10, 10, 1, 0, 12, 10, 0, 2]);
    }

    #[test]
    fn preserves_one_vertex_for_a_collapsed_polyline() {
      let geometry = polyline(vec![
        coordinate(0.1, 0.1, None, None),
        coordinate(0.2, 0.2, None, None),
      ]);
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(2, false, false), &mut quantized).unwrap();

      assert_eq!(quantized.lengths, [1]);
      assert_eq!(quantized.coordinates, [0, 0]);
    }

    #[test]
    fn preserves_one_vertex_for_a_collapsed_polygon() {
      let geometry = Geometry::new(
        GeometryType::Polygon,
        xy(&[(0.1, 0.1), (0.2, 0.2), (0.3, 0.3)]),
        vec![3],
        vec![1],
      )
      .unwrap();
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(3, false, false), &mut quantized).unwrap();

      assert_eq!(quantized.ty, GeometryType::Polygon);
      assert_eq!(quantized.lengths, [1]);
      assert_eq!(quantized.polygon_ring_counts, [1]);
      assert_eq!(quantized.coordinates, [0, 0]);
    }

    #[test]
    fn preserves_first_xyzm_for_collapsed_parts() {
      let geometry = polyline(vec![
        coordinate(0.0, 0.0, None, Some(1.0)),
        coordinate(0.1, 0.0, Some(2.0), None),
      ]);
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&geometry, &options(3, true, true), &mut quantized).unwrap();

      assert_eq!(quantized.lengths, [1]);
      assert_eq!(quantized.coordinates, [0, 0, 0, 1]);
    }

    #[test]
    fn preserves_first_vertex_when_all_parts_collapse() {
      let geometry = Geometry::new(
        GeometryType::MultiLineString,
        vec![
          coordinate(12.0, 16.0, None, Some(108.0)),
          coordinate(18.0, 12.0, Some(40.0), None),
        ],
        vec![0, 1, 0, 1],
        Vec::new(),
      )
      .unwrap();

      for (has_z, has_m, expected) in [
        (false, false, vec![1, 2]),
        (true, false, vec![1, 2, 0]),
        (false, true, vec![1, 2, 2]),
        (true, true, vec![1, 2, 0, 2]),
      ] {
        let mut options = options(2, has_z, has_m);
        options.transform.scale = [2.0, -2.0, 5.0, 4.0];
        options.transform.translate = [10.0, 20.0, 30.0, 100.0];
        let mut quantized = QuantizedGeometry::default();

        quantize_into(&geometry, &options, &mut quantized).unwrap();

        assert_eq!(quantized.ty, GeometryType::MultiLineString);
        assert_eq!(quantized.lengths, [1]);
        assert_eq!(quantized.coordinates, expected);
        assert!(quantized.polygon_ring_counts.is_empty());
      }
    }

    #[test]
    fn collapses_identical_snapped_ring_vertices_in_each_layout() {
      for (has_z, has_m) in [(false, false), (true, false), (false, true), (true, true)] {
        let geometry = Geometry::new(
          GeometryType::Polygon,
          vec![
            coordinate(0.1, 0.1, Some(0.0), Some(0.0)),
            coordinate(0.2, 0.1, Some(0.0), Some(0.0)),
            coordinate(0.2, 0.2, Some(0.0), Some(0.0)),
            coordinate(0.1, 0.2, Some(0.0), Some(0.0)),
            coordinate(0.1, 0.1, Some(0.0), Some(0.0)),
          ],
          vec![5],
          vec![1],
        )
        .unwrap();
        let mut options = options(3, has_z, has_m);
        options.tolerance = 0.0;
        let mut quantized = QuantizedGeometry::default();

        quantize_into(&geometry, &options, &mut quantized).unwrap();

        assert_eq!(quantized.lengths, [1], "has_z: {has_z}, has_m: {has_m}");
        assert_eq!(quantized.polygon_ring_counts, [1]);
        assert!(quantized.coordinates.iter().all(|&value| value == 0));
      }
    }
  }

  mod douglas_peucker {
    use super::{coordinate, douglas_peucker_indices, point_segment_distance_squared, xy};

    #[test]
    fn preserves_inputs_with_at_most_two_vertices() {
      for count in 0..=2 {
        let coordinates = xy(&[(0.0, 0.0), (2.0, 0.0)][..count]);

        assert_eq!(
          douglas_peucker_indices(&coordinates, 1.0),
          (0..count).collect::<Vec<_>>()
        );
      }
    }

    #[test]
    fn removes_collinear_interior_vertices() {
      let coordinates = xy(&[(0.0, 0.0), (1.0, 0.0), (2.0, 0.0), (3.0, 0.0)]);

      assert_eq!(douglas_peucker_indices(&coordinates, 0.0), [0, 3]);
    }

    #[test]
    fn removes_vertices_exactly_at_tolerance() {
      let coordinates = xy(&[(0.0, 0.0), (1.0, 1.0), (2.0, 0.0)]);

      assert_eq!(douglas_peucker_indices(&coordinates, 1.0), [0, 2]);
    }

    #[test]
    fn retains_vertices_above_tolerance() {
      let coordinates = xy(&[(0.0, 0.0), (1.0, 1.01), (2.0, 0.0)]);

      assert_eq!(douglas_peucker_indices(&coordinates, 1.0), [0, 1, 2]);
    }

    #[test]
    fn retains_non_collinear_vertices_at_zero_tolerance() {
      let coordinates = xy(&[(0.0, 0.0), (1.0, 0.01), (2.0, 0.0)]);

      assert_eq!(douglas_peucker_indices(&coordinates, 0.0), [0, 1, 2]);
    }

    #[test]
    fn ignores_zm_when_selecting_vertices() {
      let coordinates = [
        coordinate(0.0, 0.0, Some(0.0), Some(0.0)),
        coordinate(1.0, 0.0, Some(1000.0), Some(-1000.0)),
        coordinate(2.0, 0.0, Some(0.0), Some(0.0)),
      ];

      assert_eq!(douglas_peucker_indices(&coordinates, 0.0), [0, 2]);
    }

    #[test]
    fn emits_unique_open_path_indices_in_source_order() {
      let coordinates = xy(&[(0.0, 0.0), (1.0, 3.0), (2.0, -3.0), (3.0, 3.0), (4.0, 0.0)]);
      let retained = douglas_peucker_indices(&coordinates, 0.1);

      assert_eq!(retained, [0, 1, 2, 3, 4]);
      assert!(retained.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn preserves_closed_triangles_at_large_tolerance() {
      let coordinates = xy(&[(0.0, 0.0), (4.0, 0.0), (0.0, 4.0), (0.0, 0.0)]);

      assert_eq!(douglas_peucker_indices(&coordinates, 100.0), [0, 1, 2, 3]);
    }

    #[test]
    fn bounds_removed_vertices_by_their_retained_segments() {
      let coordinates = (0..128)
        .map(|index| coordinate(f64::from(index), f64::from(index % 7), None, None))
        .collect::<Vec<_>>();
      let tolerance = 1.5;
      let retained = douglas_peucker_indices(&coordinates, tolerance);

      assert_eq!(retained.first(), Some(&0));
      assert_eq!(retained.last(), Some(&(coordinates.len() - 1)));
      assert!(retained.len() < coordinates.len());
      for interval in retained.windows(2) {
        for index in interval[0] + 1..interval[1] {
          let distance = point_segment_distance_squared(
            coordinates[index],
            coordinates[interval[0]],
            coordinates[interval[1]],
          );

          assert!(
            distance <= tolerance * tolerance,
            "vertex: {index}, distance: {distance}"
          );
        }
      }
    }

    #[test]
    fn simplifies_large_collinear_paths_iteratively() {
      let coordinates = (0..10_000)
        .map(|index| coordinate(f64::from(index), 0.0, None, None))
        .collect::<Vec<_>>();

      assert_eq!(douglas_peucker_indices(&coordinates, 1.0), [0, 9999]);
    }

    #[test]
    fn preserves_three_unique_ring_vertices_at_large_tolerance() {
      let coordinates = xy(&[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0), (0.0, 0.0)]);
      let retained = douglas_peucker_indices(&coordinates, 100.0);

      assert!(retained.len() >= 4, "retained indices: {retained:?}");
      let mut unique_positions = retained
        .iter()
        .map(|&index| (coordinates[index].x as i64, coordinates[index].y as i64))
        .collect::<Vec<_>>();
      unique_positions.sort_unstable();
      unique_positions.dedup();
      assert!(unique_positions.len() >= 3);
    }

    #[test]
    fn preserves_the_original_closing_index() {
      let coordinates = xy(&[(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0), (0.0, 0.0)]);
      let retained = douglas_peucker_indices(&coordinates, 0.0);

      assert_eq!(retained, [0, 1, 2, 3, 4]);
    }
  }

  mod point_segment_distance {
    use super::{douglas_peucker_indices, point_segment_distance_squared, xy};

    #[test]
    fn measures_distance_beyond_the_end() {
      let coordinates = xy(&[(0.0, 0.0), (2.0, 0.0), (1.0, 0.0)]);

      assert_eq!(douglas_peucker_indices(&coordinates, 0.5), [0, 1, 2]);
      assert_eq!(
        point_segment_distance_squared(coordinates[1], coordinates[0], coordinates[2]),
        1.0
      );
    }

    #[test]
    fn measures_distance_before_the_start() {
      let coordinates = xy(&[(0.0, 0.0), (-2.0, 1.0), (1.0, 0.0)]);

      assert_eq!(
        point_segment_distance_squared(coordinates[1], coordinates[0], coordinates[2]),
        5.0
      );
    }

    #[test]
    fn measures_distance_from_coincident_endpoints() {
      let coordinates = xy(&[(1.0, 1.0), (4.0, 5.0)]);

      assert_eq!(
        point_segment_distance_squared(coordinates[1], coordinates[0], coordinates[0]),
        25.0
      );
    }
  }

  mod collinearity {
    use super::is_collinear_delta;

    #[test]
    fn accepts_only_collinear_deltas_in_the_same_direction() {
      assert!(is_collinear_delta(1, 2, 3, 6));
      assert!(!is_collinear_delta(1, 2, -3, -6));
      assert!(!is_collinear_delta(1, 2, 3, 5));
      assert!(!is_collinear_delta(0, 0, 1, 2));
    }

    #[test]
    fn compares_large_deltas_without_overflow() {
      assert!(is_collinear_delta(
        4_000_000_000,
        4_000_000_000,
        4_000_000_000,
        4_000_000_000
      ));
    }
  }

  mod buffer_reuse {
    use super::{
      Geometry, GeometryType, QuantizedGeometry, coordinate, options, polyline, quantize_into, xy,
    };

    #[test]
    fn resets_buffers_and_component_flags_between_geometries() {
      let first = polyline(vec![
        coordinate(0.0, 0.0, Some(1.0), Some(2.0)),
        coordinate(2.0, 0.0, Some(3.0), Some(4.0)),
      ]);
      let second = polyline(xy(&[(10.0, 20.0), (12.0, 20.0)]));
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&first, &options(2, true, true), &mut quantized).unwrap();
      quantize_into(&second, &options(2, false, false), &mut quantized).unwrap();

      assert_eq!(quantized.ty, GeometryType::LineString);
      assert_eq!(quantized.lengths, [2]);
      assert_eq!(quantized.coordinates, [10, 20, 12, 20]);
      assert!(quantized.polygon_ring_counts.is_empty());
      assert!(!quantized.has_z);
      assert!(!quantized.has_m);
    }

    #[test]
    fn clears_previous_output_for_empty_geometry() {
      let first = polyline(xy(&[(0.0, 0.0), (2.0, 0.0)]));
      let empty = Geometry::new(GeometryType::Polygon, Vec::new(), Vec::new(), Vec::new()).unwrap();
      let mut quantized = QuantizedGeometry::default();

      quantize_into(&first, &options(2, false, false), &mut quantized).unwrap();
      quantize_into(&empty, &options(3, false, false), &mut quantized).unwrap();

      assert_eq!(quantized.ty, GeometryType::Polygon);
      assert!(quantized.coordinates.is_empty());
      assert!(quantized.lengths.is_empty());
      assert!(quantized.polygon_ring_counts.is_empty());
    }
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
    let length = u32::try_from(coordinates.len()).unwrap();
    Geometry::new(
      GeometryType::LineString,
      coordinates,
      vec![length],
      Vec::new(),
    )
    .unwrap()
  }

  fn xy(positions: &[(f64, f64)]) -> Vec<Coord> {
    positions
      .iter()
      .map(|&(x, y)| coordinate(x, y, None, None))
      .collect()
  }

  fn coordinate(x: f64, y: f64, z: Option<f64>, m: Option<f64>) -> Coord {
    Coord { x, y, z, m }
  }
}

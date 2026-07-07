use crate::analysis::Extent2D;

pub type DisplayCode = u64;

pub const DEFAULT_XZ_MAX_LEVEL: u32 = 20;
pub const DEFAULT_COORDINATE_PRECISION: u32 = 20;

pub fn point_z_code(
  full_extent: Extent2D,
  x: f64,
  y: f64,
  coordinate_precision: u32,
) -> DisplayCode {
  let quantized_x = quantize_to_bits(x, full_extent.xmin, full_extent.xmax, coordinate_precision);
  let quantized_y = quantize_to_bits(y, full_extent.ymin, full_extent.ymax, coordinate_precision);
  swizzle_bits(quantized_x, quantized_y, coordinate_precision)
}

pub fn swizzle_bits(x: u32, y: u32, coordinate_precision: u32) -> DisplayCode {
  let mut out = 0u64;
  for bit in 0..coordinate_precision.min(32) {
    let x_bit = ((x >> bit) & 1) as u64;
    let y_bit = ((y >> bit) & 1) as u64;
    out |= x_bit << (2 * bit);
    out |= y_bit << (2 * bit + 1);
  }
  out
}

pub fn extent_xz_level(full_extent: Extent2D, feature_extent: Extent2D, max_depth: u32) -> u32 {
  let full_extent_width = full_extent.xmax - full_extent.xmin;
  let full_extent_height = full_extent.ymax - full_extent.ymin;
  let feature_width = feature_extent.xmax - feature_extent.xmin;
  let feature_height = feature_extent.ymax - feature_extent.ymin;

  if feature_width <= 0.0 || feature_height <= 0.0 {
    return 0;
  }

  let x_level = (full_extent_width / feature_width).log2();
  let y_level = (full_extent_height / feature_height).log2();
  ((x_level.min(y_level).floor() as u32) + 1).min(max_depth)
}

pub fn extent_xz_code(
  full_extent: Extent2D,
  feature_extent: Extent2D,
  max_depth: u32,
) -> DisplayCode {
  let full_extent_width = full_extent.xmax - full_extent.xmin;
  let full_extent_height = full_extent.ymax - full_extent.ymin;
  let mut level = extent_xz_level(full_extent, feature_extent, max_depth);

  let cell_width = full_extent_width / 2f64.powi(level as i32);
  let cell_height = full_extent_height / 2f64.powi(level as i32);

  let cell_x_start = ((feature_extent.xmin - full_extent.xmin) / cell_width).floor() as i32;
  let cell_x_end = ((feature_extent.xmax - full_extent.xmin) / cell_width).floor() as i32;
  let cell_y_start = ((feature_extent.ymin - full_extent.ymin) / cell_height).floor() as i32;
  let cell_y_end = ((feature_extent.ymax - full_extent.ymin) / cell_height).floor() as i32;

  let cell_count_x = cell_x_end - cell_x_start + 1;
  let cell_count_y = cell_y_end - cell_y_start + 1;

  if cell_count_x > 2 || cell_count_y > 2 {
    level = level.saturating_sub(1);
  }

  point_xz_code(
    full_extent,
    feature_extent.xmin,
    feature_extent.ymin,
    max_depth,
    Some(level),
  )
}

pub fn point_xz_code(
  full_extent: Extent2D,
  point_x: f64,
  point_y: f64,
  max_depth: u32,
  insert_level: Option<u32>,
) -> DisplayCode {
  let insert_level = insert_level.unwrap_or(max_depth);
  let mut depth = 0;
  let mut sequence_code: DisplayCode = 0;
  let mut xmin = full_extent.xmin;
  let mut ymin = full_extent.ymin;
  let mut xmax = full_extent.xmax;
  let mut ymax = full_extent.ymax;

  while depth != insert_level {
    let center_x = (xmin + xmax) / 2.0;
    let quadrant_x = if point_x >= center_x {
      xmin = center_x;
      1
    } else {
      xmax = center_x;
      0
    };

    let center_y = (ymin + ymax) / 2.0;
    let quadrant_y = if point_y >= center_y {
      ymin = center_y;
      1
    } else {
      ymax = center_y;
      0
    };

    let quadrant_code = quadrant_x | (quadrant_y << 1);
    sequence_code += code_for_level(quadrant_code, depth, max_depth);
    depth += 1;
  }

  sequence_code
}

fn quantize_to_bits(value: f64, min: f64, max: f64, coordinate_precision: u32) -> u32 {
  if coordinate_precision == 0 || (max - min).abs() < f64::EPSILON {
    return 0;
  }
  let normalized = ((value - min) / (max - min)).clamp(0.0, 1.0);
  let max_value = (1u64 << coordinate_precision.min(32)) - 1;
  (normalized * max_value as f64).round() as u32
}

fn code_for_level(quadrant_code: u32, depth: u32, max_depth: u32) -> DisplayCode {
  (quadrant_code as DisplayCode) * element_count(max_depth, depth) + 1
}

fn element_count(max_depth: u32, sequence_index: u32) -> DisplayCode {
  (4u64.pow(max_depth - sequence_index) - 1) / 3
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn swizzle_bits_interleaves_xy_bits() {
    assert_eq!(swizzle_bits(0, 0, 4), 0);
    assert_eq!(swizzle_bits(1, 0, 4), 1);
    assert_eq!(swizzle_bits(0, 1, 4), 2);
    assert_eq!(swizzle_bits(1, 1, 4), 3);
    assert_eq!(swizzle_bits(3, 3, 2), 15);
  }

  #[test]
  fn point_z_code_normalizes_against_full_extent() {
    let full_extent = Extent2D {
      xmin: -180.0,
      ymin: -90.0,
      xmax: 180.0,
      ymax: 90.0,
    };

    assert_eq!(point_z_code(full_extent, -180.0, -90.0, 4), 0);
    assert_eq!(point_z_code(full_extent, 180.0, 90.0, 2), 15);
    assert_eq!(point_z_code(full_extent, 0.0, 0.0, 1), 3);
  }

  #[test]
  fn extent_xz_code_matches_reference_cases() {
    let full_extent = Extent2D {
      xmin: 0.0,
      ymin: 0.0,
      xmax: 8.0,
      ymax: 8.0,
    };

    assert_eq!(
      extent_xz_code(
        full_extent,
        Extent2D {
          xmin: 0.0,
          ymin: 0.0,
          xmax: 1.0,
          ymax: 1.0,
        },
        1,
      ),
      1
    );
    assert_eq!(
      extent_xz_code(
        full_extent,
        Extent2D {
          xmin: 7.0,
          ymin: 7.0,
          xmax: 8.0,
          ymax: 8.0,
        },
        1,
      ),
      4
    );
    assert_eq!(
      extent_xz_code(
        full_extent,
        Extent2D {
          xmin: 0.0,
          ymin: 0.0,
          xmax: 0.9,
          ymax: 0.9,
        },
        2,
      ),
      2
    );
    assert_eq!(extent_xz_code(full_extent, full_extent, 3), 0);
    assert_eq!(
      extent_xz_code(
        full_extent,
        Extent2D {
          xmin: 3.0,
          ymin: 4.2,
          xmax: 4.9,
          ymax: 4.9,
        },
        3,
      ),
      51
    );
  }

  #[test]
  fn extent_xz_code_uses_grid_relative_to_full_extent() {
    let full_extent = Extent2D {
      xmin: -117.3150315,
      ymin: 33.989629,
      xmax: -116.9566474,
      ymax: 34.1729119,
    };
    let feature_extent = Extent2D {
      xmin: -117.0992242,
      ymin: 34.0448088,
      xmax: -117.085404,
      ymax: 34.0545866,
    };

    assert_eq!(
      extent_xz_code(full_extent, feature_extent, 20),
      555482436952
    );
  }

  #[test]
  fn point_xz_code_matches_reference_quadrants() {
    let full_extent = Extent2D {
      xmin: 2.0,
      ymin: 2.0,
      xmax: 4.0,
      ymax: 4.0,
    };
    assert_eq!(point_xz_code(full_extent, 2.0, 2.0, 1, None), 1);
    assert_eq!(point_xz_code(full_extent, 3.0, 2.0, 1, None), 2);
    assert_eq!(point_xz_code(full_extent, 2.0, 3.0, 1, None), 3);
    assert_eq!(point_xz_code(full_extent, 3.0, 3.0, 1, None), 4);
  }
}

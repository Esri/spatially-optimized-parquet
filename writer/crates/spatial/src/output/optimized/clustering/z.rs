//! Computes Morton Z-order codes for point clustering.

use crate::analysis::Extent2D;

use super::common::DisplayCode;

/// Stores the default number of quantization bits per point coordinate axis.
pub(crate) const DEFAULT_COORDINATE_PRECISION: u32 = 20;

/// Quantize a point within the full extent and interleave its x/y bits.
pub(crate) fn point_z_code(
  full_extent: Extent2D,
  x: f64,
  y: f64,
  coordinate_precision: u32,
) -> DisplayCode {
  let quantized_x = quantize_to_bits(x, full_extent.xmin, full_extent.xmax, coordinate_precision);
  let quantized_y = quantize_to_bits(y, full_extent.ymin, full_extent.ymax, coordinate_precision);
  swizzle_bits(quantized_x, quantized_y, coordinate_precision)
}

/// Interleave x and y bits into one Morton-order code.
pub(crate) fn swizzle_bits(x: u32, y: u32, coordinate_precision: u32) -> DisplayCode {
  let mut code = 0;
  for bit in 0..coordinate_precision.min(32) {
    let x_bit = ((x >> bit) & 1) as DisplayCode;
    let y_bit = ((y >> bit) & 1) as DisplayCode;
    code |= x_bit << (2 * bit);
    code |= y_bit << (2 * bit + 1);
  }
  code
}

fn quantize_to_bits(value: f64, min: f64, max: f64, coordinate_precision: u32) -> u32 {
  if coordinate_precision == 0 || (max - min).abs() < f64::EPSILON {
    return 0;
  }
  let cell_count = 1u64 << coordinate_precision.min(32);
  let normalized = (value - min) / (max - min);
  let quantized = (normalized * cell_count as f64) as i64;
  quantized.clamp(0, cell_count as i64 - 1) as u32
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
  fn point_z_code_uses_cell_quantization() {
    let full_extent = Extent2D {
      xmin: 0.0,
      ymin: 0.0,
      xmax: 1.0,
      ymax: 1.0,
    };

    assert_eq!(point_z_code(full_extent, 0.2, 0.0, 2), 0);
    assert_eq!(point_z_code(full_extent, 0.25, 0.0, 2), 1);
    assert_eq!(point_z_code(full_extent, 1.0, 1.0, 2), 15);
  }
}

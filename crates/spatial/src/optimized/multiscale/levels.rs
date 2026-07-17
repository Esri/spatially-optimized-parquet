//! Plans the multiscale geometry representations emitted for complex geometry features.

use anyhow::Result;

use crate::geometry::{GeometryType, QuantizationTransform};
use crate::output::{DEFAULT_OUTPUT_WKID, WEB_MERCATOR_OUTPUT_WKID};

const WGS84_SEMI_MAJOR_AXIS: f64 = 6_378_137.0;
const ROOT_GRID_SIZE: f64 = 512.0;
const DISPLAY_DPI: f64 = 96.0;
const WGS84_EQUATORIAL_CIRCUMFERENCE: f64 = WGS84_SEMI_MAJOR_AXIS * std::f64::consts::TAU;
/// Stores the WGS84 angular resolution used for the first multiscale level.
const FIRST_LEVEL_RESOLUTION: f64 = 360.0 / ROOT_GRID_SIZE;
/// Stores the Web Mercator resolution equivalent to the first WGS84 level.
const FIRST_PROJECTED_LEVEL_RESOLUTION: f64 = WGS84_EQUATORIAL_CIRCUMFERENCE / ROOT_GRID_SIZE;
/// Stores the WGS84 map scale denominator used for the first multiscale level.
const FIRST_LEVEL_SCALE: f64 =
  WGS84_EQUATORIAL_CIRCUMFERENCE * DISPLAY_DPI * 10_000.0 / (254.0 * ROOT_GRID_SIZE);
const MAX_MULTISCALE_LEVEL: u16 = 16;

/// Stores the quantization and simplification settings for one output geometry column.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MultiscaleLevel {
  /// Stores the multiscale level represented by the column.
  pub(crate) level: u16,
  /// Stores the generated Parquet column name.
  pub(crate) column: String,
  /// Stores the coordinate resolution at this level.
  pub(crate) resolution: f64,
  /// Stores the map scale denominator at this level.
  pub(crate) scale: f64,
  /// Stores the quantization transform applied before Esri PBF encoding.
  pub(crate) transform: QuantizationTransform,
  /// Stores the minimum retained vertex count for the geometry family.
  pub(crate) min_length: usize,
}

/// Create supported even-numbered multiscale level specifications for the target spatial reference.
pub(crate) fn create_multiscale_levels(
  output_wkid: u32,
  geometry_type: GeometryType,
) -> Result<Vec<MultiscaleLevel>> {
  let min_length = min_vertex_count(geometry_type);
  let mut resolution = match output_wkid {
    DEFAULT_OUTPUT_WKID => FIRST_LEVEL_RESOLUTION,
    WEB_MERCATOR_OUTPUT_WKID => FIRST_PROJECTED_LEVEL_RESOLUTION,
    _ => todo!("multiscale levels for output WKID {output_wkid}"),
  };
  let mut scale = FIRST_LEVEL_SCALE;
  let mut levels = Vec::new();

  for level in 0..=MAX_MULTISCALE_LEVEL {
    if level % 2 == 0 {
      levels.push(MultiscaleLevel {
        level,
        column: format!("level_{level}"),
        resolution,
        scale,
        transform: QuantizationTransform {
          scale: [resolution, resolution, 1.0, 1.0],
          translate: [0.0, 0.0, 0.0, 0.0],
        },
        min_length,
      });
    }
    resolution /= 2.0;
    scale /= 2.0;
  }

  Ok(levels)
}

fn min_vertex_count(geometry_type: GeometryType) -> usize {
  match geometry_type {
    GeometryType::MultiPoint | GeometryType::Point => 1,
    GeometryType::Polyline => 2,
    GeometryType::Polygon => 3,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn creates_even_wgs84_levels() {
    let levels = create_multiscale_levels(DEFAULT_OUTPUT_WKID, GeometryType::Polygon).unwrap();
    assert_eq!(levels.first().unwrap().level, 0);
    assert_eq!(levels.last().unwrap().level, 16);
    assert_eq!(levels[0].min_length, 3);
    assert_eq!(levels.len(), 9);
    assert_eq!(levels[0].resolution, FIRST_LEVEL_RESOLUTION);
    assert_eq!(levels[0].scale, FIRST_LEVEL_SCALE);
    assert_eq!(
      levels[0].transform.scale,
      [FIRST_LEVEL_RESOLUTION, FIRST_LEVEL_RESOLUTION, 1.0, 1.0]
    );
    assert_eq!(levels[1].level, 2);
    assert_eq!(levels[1].resolution, FIRST_LEVEL_RESOLUTION / 4.0);
  }

  #[test]
  fn creates_web_mercator_levels() {
    let levels = create_multiscale_levels(WEB_MERCATOR_OUTPUT_WKID, GeometryType::Polygon).unwrap();
    assert_eq!(levels[0].resolution, FIRST_PROJECTED_LEVEL_RESOLUTION);
    assert_eq!(
      levels[0].transform.scale,
      [
        FIRST_PROJECTED_LEVEL_RESOLUTION,
        FIRST_PROJECTED_LEVEL_RESOLUTION,
        1.0,
        1.0
      ]
    );
    assert_eq!(levels[1].resolution, FIRST_PROJECTED_LEVEL_RESOLUTION / 4.0);
  }
}

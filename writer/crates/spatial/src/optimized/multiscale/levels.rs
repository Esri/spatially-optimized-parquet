//! Plans the multiscale geometry representations emitted for non-point features.

use anyhow::Result;
use serde::Serialize;

use crate::optimized::OptimizedGeometryType;
use crate::output::{DEFAULT_OUTPUT_WKID, WEB_MERCATOR_OUTPUT_WKID};

/// Stores the maximum display hierarchy level advertised in output metadata.
pub const DEFAULT_MAX_LEVEL: u32 = 20;
/// Stores the WGS84 angular resolution used for the first multiscale level.
const FIRST_LEVEL_RESOLUTION: f64 = 0.70312359375;
/// Stores the Web Mercator resolution equivalent to the first WGS84 level.
const FIRST_PROJECTED_LEVEL_RESOLUTION: f64 = 78_271.360_420_986_54;
/// Stores the WGS84 map scale denominator used for the first multiscale level.
const FIRST_LEVEL_SCALE: f64 = 295_828_763.795_854_7;
const MAX_MULTISCALE_LEVEL: u16 = 16;

#[derive(Debug, Clone, PartialEq, Serialize)]
/// Describes one generated multiscale geometry column.
pub struct MultiscaleLevel {
  /// Names the generated payload column.
  pub column: String,
  /// Stores the display level.
  pub level: u16,
  /// Stores the coordinate resolution.
  pub resolution: f64,
  /// Stores the map scale denominator.
  pub scale: f64,
  /// Stores the payload quantization transform.
  pub transform: QuantizationTransform,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
/// Describes the scale and translation used to quantize four-dimensional coordinates.
pub struct QuantizationTransform {
  /// Stores per-axis quantization scale values.
  pub scale: [f64; 4],
  /// Stores per-axis quantization origins.
  pub translate: [f64; 4],
}

/// Stores the quantization and simplification settings for one output geometry column.
#[derive(Debug, Clone, PartialEq)]
pub struct GeometryEncoding {
  /// Stores the display level represented by the column.
  pub level: u16,
  /// Stores the generated Parquet column name.
  pub column: String,
  /// Stores the coordinate resolution at this level.
  pub resolution: f64,
  /// Stores the map scale denominator at this level.
  pub scale: f64,
  /// Stores the quantization transform applied before PBF encoding.
  pub transform: QuantizationTransform,
  /// Stores the minimum retained vertex count for the geometry family.
  pub min_length: usize,
}

/// Build the supported even-numbered display encodings for the target spatial reference.
pub fn create_geometry_encodings(
  output_wkid: u32,
  geometry_type: OptimizedGeometryType,
) -> Result<Vec<GeometryEncoding>> {
  let min_length = min_vertex_count(geometry_type);
  let mut resolution = match output_wkid {
    DEFAULT_OUTPUT_WKID => FIRST_LEVEL_RESOLUTION,
    WEB_MERCATOR_OUTPUT_WKID => FIRST_PROJECTED_LEVEL_RESOLUTION,
    _ => todo!("multiscale levels for output WKID {output_wkid}"),
  };
  let mut scale = FIRST_LEVEL_SCALE;
  let mut encodings = Vec::new();

  for level in 0..=MAX_MULTISCALE_LEVEL {
    if level % 2 == 0 {
      encodings.push(GeometryEncoding {
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

  Ok(encodings)
}

/// Convert runtime encoding plans into serializable output metadata.
pub fn metadata_levels(encodings: &[GeometryEncoding]) -> Vec<MultiscaleLevel> {
  encodings
    .iter()
    .map(|encoding| MultiscaleLevel {
      column: encoding.column.clone(),
      level: encoding.level,
      resolution: encoding.resolution,
      scale: encoding.scale,
      transform: encoding.transform.clone(),
    })
    .collect()
}

pub(super) fn min_vertex_count(geometry_type: OptimizedGeometryType) -> usize {
  match geometry_type {
    OptimizedGeometryType::MultiPoint | OptimizedGeometryType::Point => 1,
    OptimizedGeometryType::Polyline => 2,
    OptimizedGeometryType::Polygon => 3,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn creates_even_wgs84_levels() {
    let encodings =
      create_geometry_encodings(DEFAULT_OUTPUT_WKID, OptimizedGeometryType::Polygon).unwrap();
    assert_eq!(encodings.first().unwrap().level, 0);
    assert_eq!(encodings.last().unwrap().level, 16);
    assert_eq!(encodings[0].min_length, 3);
    assert_eq!(encodings.len(), 9);
    assert_eq!(encodings[0].resolution, FIRST_LEVEL_RESOLUTION);
    assert_eq!(encodings[0].scale, FIRST_LEVEL_SCALE);
    assert_eq!(
      encodings[0].transform.scale,
      [FIRST_LEVEL_RESOLUTION, FIRST_LEVEL_RESOLUTION, 1.0, 1.0]
    );
    assert_eq!(encodings[1].level, 2);
    assert_eq!(encodings[1].resolution, FIRST_LEVEL_RESOLUTION / 4.0);
  }

  #[test]
  fn creates_web_mercator_levels() {
    let encodings =
      create_geometry_encodings(WEB_MERCATOR_OUTPUT_WKID, OptimizedGeometryType::Polygon).unwrap();
    assert_eq!(encodings[0].resolution, FIRST_PROJECTED_LEVEL_RESOLUTION);
    assert_eq!(
      encodings[0].transform.scale,
      [
        FIRST_PROJECTED_LEVEL_RESOLUTION,
        FIRST_PROJECTED_LEVEL_RESOLUTION,
        1.0,
        1.0
      ]
    );
    assert_eq!(
      encodings[1].resolution,
      FIRST_PROJECTED_LEVEL_RESOLUTION / 4.0
    );
  }
}

//! Plans the multiscale geometry representations emitted for non-point features.

use anyhow::{Result, bail};

use crate::analysis::DisplayGeometryType;
use crate::metadata::output::{MultiscaleLevel, QuantizationTransform};

/// Stores the maximum display hierarchy level advertised in output metadata.
pub const DEFAULT_MAX_LEVEL: u32 = 20;
/// Stores the only coordinate system currently supported for display payload output.
pub const DISPLAY_OUTPUT_WKID: u32 = 4326;
const FIRST_LEVEL_RESOLUTION: f64 = 0.70312359375;
const FIRST_LEVEL_SCALE: f64 = 295_828_763.795_854_7;
const MAX_MULTISCALE_LEVEL: u16 = 16;

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

/// Build the supported even-numbered display encodings for WGS84 output.
pub fn create_geometry_encodings(
  output_wkid: u32,
  geometry_type: DisplayGeometryType,
) -> Result<Vec<GeometryEncoding>> {
  if output_wkid != DISPLAY_OUTPUT_WKID {
    bail!(
      "multiscale display optimization currently only supports output WKID {}",
      DISPLAY_OUTPUT_WKID
    );
  }

  let min_length = min_vertex_count(geometry_type);
  let mut resolution = FIRST_LEVEL_RESOLUTION;
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

pub(super) fn min_vertex_count(geometry_type: DisplayGeometryType) -> usize {
  match geometry_type {
    DisplayGeometryType::MultiPoint | DisplayGeometryType::Point => 1,
    DisplayGeometryType::Polyline => 2,
    DisplayGeometryType::Polygon => 3,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn creates_even_levels() {
    let encodings =
      create_geometry_encodings(DISPLAY_OUTPUT_WKID, DisplayGeometryType::Polygon).unwrap();
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
  fn rejects_non_wgs84_output() {
    let error = create_geometry_encodings(3857, DisplayGeometryType::Polygon).unwrap_err();
    assert!(
      error
        .to_string()
        .contains("currently only supports output WKID 4326")
    );
  }
}

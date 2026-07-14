//! Defines the geodisplay JSON contract.

use serde::Serialize;

use crate::geometry::Extent2D;

use super::parquet::ParquetMetadata;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct GeodisplayMetadata {
  #[serde(rename = "parentColumn")]
  parent_column: Option<String>,
  index: ClusteringIndex,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
enum ClusteringIndex {
  Z(ZClusteringIndex),
  Xz(XzClusteringIndex),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct ZClusteringIndex {
  #[serde(rename = "type")]
  index_type: &'static str,
  code: String,
  wkid: Option<u32>,
  wkt: Option<String>,
  #[serde(rename = "xColumn")]
  x_column: String,
  #[serde(rename = "yColumn")]
  y_column: String,
  #[serde(rename = "zColumn", skip_serializing_if = "Option::is_none")]
  z_column: Option<String>,
  #[serde(rename = "mColumn", skip_serializing_if = "Option::is_none")]
  m_column: Option<String>,
  #[serde(rename = "coordinatePrecision")]
  coordinate_precision: u32,
  #[serde(rename = "fullExtent")]
  full_extent: Extent2D,
  #[serde(rename = "hasZ")]
  has_z: bool,
  #[serde(rename = "hasM")]
  has_m: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct XzClusteringIndex {
  #[serde(rename = "type")]
  index_type: &'static str,
  code: String,
  wkid: Option<u32>,
  wkt: Option<String>,
  encoding: String,
  #[serde(rename = "geometryType")]
  geometry_type: String,
  bounds: String,
  #[serde(rename = "fullExtent")]
  full_extent: Extent2D,
  #[serde(rename = "maxLevel")]
  max_level: u32,
  #[serde(rename = "hasZ")]
  has_z: bool,
  #[serde(rename = "hasM")]
  has_m: bool,
  levels: Vec<MultiscaleLevel>,
}

/// Stores point index values before fixed metadata fields are applied.
pub(crate) struct ZClusteringIndexInput {
  /// Names the Z-order code column.
  pub(crate) code: String,
  /// Names the x-coordinate column.
  pub(crate) x_column: String,
  /// Names the y-coordinate column.
  pub(crate) y_column: String,
  /// Stores coordinate quantization precision.
  pub(crate) coordinate_precision: u32,
  /// Stores the indexed dataset extent.
  pub(crate) full_extent: Extent2D,
  /// Stores the coordinate reference authority code.
  pub(crate) wkid: Option<u32>,
  /// Stores the coordinate reference WKT.
  pub(crate) wkt: Option<String>,
  /// Indicates whether coordinates contain Z ordinates.
  pub(crate) has_z: bool,
  /// Indicates whether coordinates contain M ordinates.
  pub(crate) has_m: bool,
}

/// Stores non-point index values before fixed metadata fields are applied.
pub(crate) struct XzClusteringIndexInput {
  /// Names the XZ-order code field.
  pub(crate) code: String,
  /// Names the geometry payload encoding.
  pub(crate) encoding: String,
  /// Names the geodisplay geometry category.
  pub(crate) geometry_type: String,
  /// Names the feature bounds field.
  pub(crate) bounds: String,
  /// Stores the indexed dataset extent.
  pub(crate) full_extent: Extent2D,
  /// Stores the maximum XZ hierarchy depth.
  pub(crate) max_level: u32,
  /// Stores the coordinate reference authority code.
  pub(crate) wkid: Option<u32>,
  /// Stores the coordinate reference WKT.
  pub(crate) wkt: Option<String>,
  /// Indicates whether coordinates contain Z ordinates.
  pub(crate) has_z: bool,
  /// Indicates whether coordinates contain M ordinates.
  pub(crate) has_m: bool,
  /// Lists generated multiscale geometry columns.
  pub(crate) levels: Vec<MultiscaleLevelInput>,
}

/// Stores one multiscale level before serialization fields are assembled.
pub(crate) struct MultiscaleLevelInput {
  /// Names the generated payload column.
  pub(crate) column: String,
  /// Stores the multiscale level.
  pub(crate) level: u16,
  /// Stores the coordinate resolution.
  pub(crate) resolution: f64,
  /// Stores the map scale denominator.
  pub(crate) scale: f64,
  /// Stores per-axis quantization scale values.
  pub(crate) transform_scale: [f64; 4],
  /// Stores per-axis quantization origins.
  pub(crate) transform_translate: [f64; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct MultiscaleLevel {
  column: String,
  level: u16,
  resolution: f64,
  scale: f64,
  transform: QuantizationTransform,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct QuantizationTransform {
  scale: [f64; 4],
  translate: [f64; 4],
}

impl GeodisplayMetadata {
  pub(super) fn point(index: ZClusteringIndex) -> Self {
    Self {
      parent_column: None,
      index: ClusteringIndex::Z(index),
    }
  }

  pub(super) fn xz_with_parent(parent_column: &str, index: XzClusteringIndex) -> Self {
    Self {
      parent_column: Some(parent_column.to_string()),
      index: ClusteringIndex::Xz(index),
    }
  }
}

impl ParquetMetadata for GeodisplayMetadata {
  const KEY: &'static str = "geodisplay";
}

impl ZClusteringIndex {
  pub(super) fn new(input: ZClusteringIndexInput) -> Self {
    Self {
      index_type: "z",
      code: input.code,
      wkid: input.wkid,
      wkt: input.wkt,
      x_column: input.x_column,
      y_column: input.y_column,
      z_column: None,
      m_column: None,
      coordinate_precision: input.coordinate_precision,
      full_extent: input.full_extent,
      has_z: input.has_z,
      has_m: input.has_m,
    }
  }
}

impl XzClusteringIndex {
  pub(super) fn new(input: XzClusteringIndexInput) -> Self {
    Self {
      index_type: "xz",
      code: input.code,
      wkid: input.wkid,
      wkt: input.wkt,
      encoding: input.encoding,
      geometry_type: input.geometry_type,
      bounds: input.bounds,
      full_extent: input.full_extent,
      max_level: input.max_level,
      has_z: input.has_z,
      has_m: input.has_m,
      levels: input
        .levels
        .into_iter()
        .map(|level| MultiscaleLevel {
          column: level.column,
          level: level.level,
          resolution: level.resolution,
          scale: level.scale,
          transform: QuantizationTransform {
            scale: level.transform_scale,
            translate: level.transform_translate,
          },
        })
        .collect(),
    }
  }
}

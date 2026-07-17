//! Defines and serializes the Geodisplay JSON metadata contract.

use ::parquet::file::metadata::KeyValue;
use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::geometry::{Extent2D, QuantizationTransform};

pub(crate) const GEODISPLAY_VERSION: &str = "0.1";
pub(crate) const ESRI_PBF_ENCODING: &str = "esriPBF";
pub(crate) const QUANTIZED_NATIVE_ENCODING: &str = "quantizedNative";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct GeodisplayMetadata {
  #[serde(rename = "parentColumn")]
  pub(crate) parent_column: Option<String>,
  pub(crate) index: GeodisplayIndex,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum GeodisplayIndex {
  Z(ZClusteringIndex),
  Xz(XzClusteringIndex),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ZClusteringIndex {
  #[serde(rename = "type")]
  pub(crate) index_type: String,
  pub(crate) version: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) writer: Option<WriterMetadata>,
  pub(crate) code: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) wkid: Option<u32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) wkt: Option<String>,
  #[serde(rename = "xColumn")]
  pub(crate) x_column: String,
  #[serde(rename = "yColumn")]
  pub(crate) y_column: String,
  #[serde(rename = "zColumn", skip_serializing_if = "Option::is_none")]
  pub(crate) z_column: Option<String>,
  #[serde(rename = "mColumn", skip_serializing_if = "Option::is_none")]
  pub(crate) m_column: Option<String>,
  #[serde(rename = "coordinatePrecision")]
  pub(crate) coordinate_precision: u32,
  #[serde(rename = "fullExtent")]
  pub(crate) full_extent: Extent2D,
  #[serde(rename = "geometryType")]
  pub(crate) geometry_type: String,
  #[serde(rename = "hasZ")]
  pub(crate) has_z: bool,
  #[serde(rename = "hasM")]
  pub(crate) has_m: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct XzClusteringIndex {
  #[serde(rename = "type")]
  pub(crate) index_type: String,
  pub(crate) version: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) writer: Option<WriterMetadata>,
  pub(crate) code: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) wkid: Option<u32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) wkt: Option<String>,
  pub(crate) encoding: String,
  #[serde(rename = "geometryType")]
  pub(crate) geometry_type: String,
  #[serde(rename = "fullExtent")]
  pub(crate) full_extent: Extent2D,
  #[serde(rename = "maxLevel")]
  pub(crate) max_level: u32,
  #[serde(rename = "hasZ")]
  pub(crate) has_z: bool,
  #[serde(rename = "hasM")]
  pub(crate) has_m: bool,
  pub(crate) levels: Vec<MultiscaleLevel>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct WriterMetadata {
  pub(crate) name: String,
  pub(crate) version: String,
}

/// Stores point index values before fixed metadata fields are applied.
pub(crate) struct ZClusteringIndexInput {
  /// Names the Z-order code column.
  pub(crate) code: String,
  /// Names the x-coordinate column.
  pub(crate) x_column: String,
  /// Names the y-coordinate column.
  pub(crate) y_column: String,
  /// Names the z-coordinate column when present.
  pub(crate) z_column: Option<String>,
  /// Names the m-coordinate column when present.
  pub(crate) m_column: Option<String>,
  /// Stores coordinate quantization precision.
  pub(crate) coordinate_precision: u32,
  /// Stores the indexed dataset extent.
  pub(crate) full_extent: Extent2D,
  /// Stores the coordinate reference authority code.
  pub(crate) wkid: Option<u32>,
  /// Stores the coordinate reference WKT.
  pub(crate) wkt: Option<String>,
  /// Indicates whether coordinates contain Z values.
  pub(crate) has_z: bool,
  /// Indicates whether coordinates contain M values.
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
  /// Stores the indexed dataset extent.
  pub(crate) full_extent: Extent2D,
  /// Stores the maximum XZ hierarchy depth.
  pub(crate) max_level: u32,
  /// Stores the coordinate reference authority code.
  pub(crate) wkid: Option<u32>,
  /// Stores the coordinate reference WKT.
  pub(crate) wkt: Option<String>,
  /// Indicates whether coordinates contain Z values.
  pub(crate) has_z: bool,
  /// Indicates whether coordinates contain M values.
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct MultiscaleLevel {
  pub(crate) column: String,
  pub(crate) level: u16,
  pub(crate) resolution: f64,
  pub(crate) scale: f64,
  pub(crate) transform: QuantizationTransform,
}

impl GeodisplayMetadata {
  pub(super) fn point(parent_column: &str, index: ZClusteringIndex) -> Self {
    Self {
      parent_column: Some(parent_column.to_string()),
      index: GeodisplayIndex::Z(index),
    }
  }

  pub(super) fn xz(parent_column: &str, index: XzClusteringIndex) -> Self {
    Self {
      parent_column: Some(parent_column.to_string()),
      index: GeodisplayIndex::Xz(index),
    }
  }
}

/// Serialize one Geodisplay metadata entry.
pub(crate) fn geodisplay_metadata_entry(metadata: &GeodisplayMetadata) -> Result<KeyValue> {
  Ok(KeyValue::new(
    "geodisplay".to_string(),
    Some(serde_json::to_string(metadata)?),
  ))
}

impl ZClusteringIndex {
  pub(super) fn new(input: ZClusteringIndexInput) -> Self {
    Self {
      index_type: "z".to_string(),
      version: GEODISPLAY_VERSION.to_string(),
      writer: None,
      code: input.code,
      wkid: input.wkid,
      wkt: input.wkt,
      x_column: input.x_column,
      y_column: input.y_column,
      z_column: input.z_column,
      m_column: input.m_column,
      coordinate_precision: input.coordinate_precision,
      full_extent: input.full_extent,
      geometry_type: "point".to_string(),
      has_z: input.has_z,
      has_m: input.has_m,
    }
  }
}

impl XzClusteringIndex {
  pub(super) fn new(input: XzClusteringIndexInput) -> Self {
    Self {
      index_type: "xz".to_string(),
      version: GEODISPLAY_VERSION.to_string(),
      writer: None,
      code: input.code,
      wkid: input.wkid,
      wkt: input.wkt,
      encoding: input.encoding,
      geometry_type: input.geometry_type,
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

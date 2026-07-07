use serde::Serialize;
use serde_json::Value;

use crate::analysis::Extent2D;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GeodisplayMetadata {
  #[serde(rename = "parentColumn")]
  pub parent_column: Option<String>,
  pub index: DisplayIndex,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum DisplayIndex {
  Z(DisplayIndexZ),
  Xz(DisplayIndexXz),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DisplayIndexZ {
  #[serde(rename = "type")]
  pub index_type: &'static str,
  pub code: String,
  pub wkid: Option<u32>,
  pub wkt: Option<String>,
  #[serde(rename = "xColumn")]
  pub x_column: String,
  #[serde(rename = "yColumn")]
  pub y_column: String,
  #[serde(rename = "zColumn", skip_serializing_if = "Option::is_none")]
  pub z_column: Option<String>,
  #[serde(rename = "mColumn", skip_serializing_if = "Option::is_none")]
  pub m_column: Option<String>,
  #[serde(rename = "coordinatePrecision")]
  pub coordinate_precision: u32,
  #[serde(rename = "fullExtent")]
  pub full_extent: Extent2D,
  #[serde(rename = "hasZ")]
  pub has_z: bool,
  #[serde(rename = "hasM")]
  pub has_m: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DisplayIndexXz {
  #[serde(rename = "type")]
  pub index_type: &'static str,
  pub code: String,
  pub wkid: Option<u32>,
  pub wkt: Option<String>,
  pub encoding: String,
  #[serde(rename = "geometryType")]
  pub geometry_type: String,
  pub bounds: String,
  #[serde(rename = "fullExtent")]
  pub full_extent: Extent2D,
  #[serde(rename = "maxLevel")]
  pub max_level: u32,
  #[serde(rename = "hasZ")]
  pub has_z: bool,
  #[serde(rename = "hasM")]
  pub has_m: bool,
  pub levels: Vec<MultiscaleLevel>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MultiscaleLevel {
  pub column: String,
  pub level: u16,
  pub resolution: f64,
  pub scale: f64,
  pub transform: QuantizationTransform,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuantizationTransform {
  pub scale: [f64; 4],
  pub translate: [f64; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpatialReferenceMetadata {
  pub wkid: Option<u32>,
  pub wkt: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub projjson: Option<Value>,
}

impl GeodisplayMetadata {
  pub fn point(index: DisplayIndexZ) -> Self {
    Self {
      parent_column: None,
      index: DisplayIndex::Z(index),
    }
  }

  pub fn xz(index: DisplayIndexXz) -> Self {
    Self {
      parent_column: None,
      index: DisplayIndex::Xz(index),
    }
  }

  pub fn xz_with_parent(parent_column: impl Into<String>, index: DisplayIndexXz) -> Self {
    Self {
      parent_column: Some(parent_column.into()),
      index: DisplayIndex::Xz(index),
    }
  }
}

impl DisplayIndexZ {
  pub fn new(
    code: impl Into<String>,
    x_column: impl Into<String>,
    y_column: impl Into<String>,
    coordinate_precision: u32,
    full_extent: Extent2D,
    wkid: Option<u32>,
    wkt: Option<String>,
    has_z: bool,
    has_m: bool,
  ) -> Self {
    Self {
      index_type: "z",
      code: code.into(),
      wkid,
      wkt,
      x_column: x_column.into(),
      y_column: y_column.into(),
      z_column: None,
      m_column: None,
      coordinate_precision,
      full_extent,
      has_z,
      has_m,
    }
  }
}

impl DisplayIndexXz {
  pub fn new(
    code: impl Into<String>,
    encoding: impl Into<String>,
    geometry_type: impl Into<String>,
    bounds: impl Into<String>,
    full_extent: Extent2D,
    max_level: u32,
    wkid: Option<u32>,
    wkt: Option<String>,
    has_z: bool,
    has_m: bool,
    levels: Vec<MultiscaleLevel>,
  ) -> Self {
    Self {
      index_type: "xz",
      code: code.into(),
      wkid,
      wkt,
      encoding: encoding.into(),
      geometry_type: geometry_type.into(),
      bounds: bounds.into(),
      full_extent,
      max_level,
      has_z,
      has_m,
      levels,
    }
  }
}

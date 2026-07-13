//! Defines the strongly typed metadata serialized into optimized Parquet files.
//!
//! Point output uses [`DisplayIndexZ`] to describe the Morton code and generated coordinate
//! columns. Non-point output uses [`DisplayIndexXz`] to describe XZ order, bounds, geometry
//! encoding, and every multiscale level. [`GeodisplayMetadata::xz_with_parent`] links nested
//! display data back to its source geometry column.
//!
//! Serde field renames implement the external camel-case geodisplay contract while Rust fields
//! retain idiomatic names. Constructors fix index discriminators to supported values, preventing
//! callers from emitting structurally valid but semantically inconsistent metadata.

use crate::analysis::Extent2D;
use crate::optimized::multiscale::MultiscaleLevel;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
/// Represents the root geodisplay metadata attached to an optimized geometry representation.
pub struct GeodisplayMetadata {
  #[serde(rename = "parentColumn")]
  /// Names the source column represented by nested display metadata.
  pub parent_column: Option<String>,
  /// Stores point or non-point index metadata.
  pub index: DisplayIndex,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
/// Selects point Z-order or non-point XZ-order display indexing metadata.
pub enum DisplayIndex {
  /// Stores point Z-order metadata.
  Z(DisplayIndexZ),
  /// Stores non-point XZ-order metadata.
  Xz(DisplayIndexXz),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
/// Describes a point display index and its coordinate columns.
pub struct DisplayIndexZ {
  #[serde(rename = "type")]
  /// Stores the fixed `z` index discriminator.
  pub index_type: &'static str,
  /// Names the Z-order code column.
  pub code: String,
  /// Stores the coordinate reference EPSG code.
  pub wkid: Option<u32>,
  /// Stores the coordinate reference WKT.
  pub wkt: Option<String>,
  #[serde(rename = "xColumn")]
  /// Names the x-coordinate column.
  pub x_column: String,
  #[serde(rename = "yColumn")]
  /// Names the y-coordinate column.
  pub y_column: String,
  #[serde(rename = "zColumn", skip_serializing_if = "Option::is_none")]
  /// Names an optional z-coordinate column.
  pub z_column: Option<String>,
  #[serde(rename = "mColumn", skip_serializing_if = "Option::is_none")]
  /// Names an optional m-coordinate column.
  pub m_column: Option<String>,
  #[serde(rename = "coordinatePrecision")]
  /// Stores the bit precision used to quantize each coordinate axis.
  pub coordinate_precision: u32,
  #[serde(rename = "fullExtent")]
  /// Stores the indexed dataset extent.
  pub full_extent: Extent2D,
  #[serde(rename = "hasZ")]
  /// Indicates whether indexed coordinates include Z.
  pub has_z: bool,
  #[serde(rename = "hasM")]
  /// Indicates whether indexed coordinates include M.
  pub has_m: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
/// Describes a non-point display index, bounds column, and multiscale payloads.
pub struct DisplayIndexXz {
  #[serde(rename = "type")]
  /// Stores the fixed `xz` index discriminator.
  pub index_type: &'static str,
  /// Names the XZ-order code field.
  pub code: String,
  /// Stores the coordinate reference EPSG code.
  pub wkid: Option<u32>,
  /// Stores the coordinate reference WKT.
  pub wkt: Option<String>,
  /// Names the geometry payload encoding.
  pub encoding: String,
  #[serde(rename = "geometryType")]
  /// Names the display geometry category.
  pub geometry_type: String,
  /// Names the feature bounds field.
  pub bounds: String,
  #[serde(rename = "fullExtent")]
  /// Stores the indexed dataset extent.
  pub full_extent: Extent2D,
  #[serde(rename = "maxLevel")]
  /// Stores the maximum XZ hierarchy depth.
  pub max_level: u32,
  #[serde(rename = "hasZ")]
  /// Indicates whether encoded coordinates include Z.
  pub has_z: bool,
  #[serde(rename = "hasM")]
  /// Indicates whether encoded coordinates include M.
  pub has_m: bool,
  /// Lists generated multiscale geometry columns.
  pub levels: Vec<MultiscaleLevel>,
}

impl GeodisplayMetadata {
  /// Build metadata for a point Z-order index.
  pub fn point(index: DisplayIndexZ) -> Self {
    Self {
      parent_column: None,
      index: DisplayIndex::Z(index),
    }
  }

  /// Build root metadata for a non-point XZ-order index.
  pub fn xz(index: DisplayIndexXz) -> Self {
    Self {
      parent_column: None,
      index: DisplayIndex::Xz(index),
    }
  }

  /// Build nested metadata for an XZ-order representation derived from a parent column.
  pub fn xz_with_parent(parent_column: impl Into<String>, index: DisplayIndexXz) -> Self {
    Self {
      parent_column: Some(parent_column.into()),
      index: DisplayIndex::Xz(index),
    }
  }
}

impl DisplayIndexZ {
  /// Build point-index metadata with fixed `z` index semantics.
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
  /// Build non-point index metadata with fixed `xz` index semantics.
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

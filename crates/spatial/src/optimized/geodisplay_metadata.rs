//! Defines and serializes the Geodisplay JSON metadata contract.

use ::parquet::file::metadata::KeyValue;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::geometry::{Extent2D, GeometryType, QuantizationTransform};

use super::multiscale::MultiscaleEncoding;

pub(crate) const GEODISPLAY_VERSION: &str = "0.1";
const SOP_WRITER_NAME: &str = "sop";
const SOP_WRITER_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub(crate) enum ColumnPath {
  Root(String),
  Nested([String; 2]),
}

impl ColumnPath {
  pub(crate) fn nested(parent: impl Into<String>, column: impl Into<String>) -> Self {
    Self::Nested([parent.into(), column.into()])
  }

  pub(crate) fn dotted(&self) -> String {
    match self {
      Self::Root(column) => column.clone(),
      Self::Nested([parent, column]) => format!("{parent}.{column}"),
    }
  }

  pub(crate) fn is_empty(&self) -> bool {
    match self {
      Self::Root(column) => column.is_empty(),
      Self::Nested([parent, column]) => parent.is_empty() || column.is_empty(),
    }
  }
}

impl fmt::Display for ColumnPath {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(&self.dotted())
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub(crate) enum GeodisplayMetadata {
  #[serde(rename = "z")]
  Z {
    #[serde(flatten)]
    index: ClusteringIndexZ,
  },
  #[serde(rename = "xz")]
  Xz {
    #[serde(flatten)]
    index: ClusteringIndexXZ,
  },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum GeodisplayEncoding {
  #[serde(rename = "esriPBF")]
  EsriPbf,
  #[serde(rename = "quantizedNative")]
  QuantizedNative,
}

impl GeodisplayEncoding {
  pub(crate) const fn as_str(self) -> &'static str {
    match self {
      Self::EsriPbf => "esriPBF",
      Self::QuantizedNative => "quantizedNative",
    }
  }
}

impl From<MultiscaleEncoding> for GeodisplayEncoding {
  fn from(encoding: MultiscaleEncoding) -> Self {
    match encoding {
      MultiscaleEncoding::Pbf => Self::EsriPbf,
      MultiscaleEncoding::QuantizedNative => Self::QuantizedNative,
    }
  }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ClusteringIndexZ {
  pub(crate) version: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) writer: Option<WriterMetadata>,
  pub(crate) code: ColumnPath,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) wkid: Option<u32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) wkt: Option<String>,
  #[serde(rename = "xColumn")]
  pub(crate) x_column: ColumnPath,
  #[serde(rename = "yColumn")]
  pub(crate) y_column: ColumnPath,
  #[serde(rename = "zColumn", skip_serializing_if = "Option::is_none")]
  pub(crate) z_column: Option<ColumnPath>,
  #[serde(rename = "mColumn", skip_serializing_if = "Option::is_none")]
  pub(crate) m_column: Option<ColumnPath>,
  #[serde(rename = "coordinatePrecision")]
  pub(crate) coordinate_precision: u32,
  #[serde(rename = "fullExtent")]
  pub(crate) full_extent: Extent2D,
  #[serde(rename = "geometryType")]
  pub(crate) geometry_type: GeometryType,
  #[serde(rename = "hasZ")]
  pub(crate) has_z: bool,
  #[serde(rename = "hasM")]
  pub(crate) has_m: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ClusteringIndexXZ {
  pub(crate) version: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) writer: Option<WriterMetadata>,
  pub(crate) code: ColumnPath,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) wkid: Option<u32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) wkt: Option<String>,
  pub(crate) encoding: GeodisplayEncoding,
  #[serde(rename = "geometryType")]
  pub(crate) geometry_type: GeometryType,
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

/// Collects point-index inputs before fixed metadata fields are applied.
pub(crate) struct ClusteringIndexZInput {
  /// Identifies the column containing Z-order codes.
  pub(crate) code: ColumnPath,
  /// Identifies the column containing x coordinates.
  pub(crate) x_column: ColumnPath,
  /// Identifies the column containing y coordinates.
  pub(crate) y_column: ColumnPath,
  /// Identifies the column containing z coordinates when present.
  pub(crate) z_column: Option<ColumnPath>,
  /// Identifies the column containing m coordinates when present.
  pub(crate) m_column: Option<ColumnPath>,
  /// Specifies the coordinate quantization precision.
  pub(crate) coordinate_precision: u32,
  /// Defines the indexed dataset extent.
  pub(crate) full_extent: Extent2D,
  /// Provides the coordinate reference authority code.
  pub(crate) wkid: Option<u32>,
  /// Provides the coordinate reference WKT.
  pub(crate) wkt: Option<String>,
  /// Indicates whether coordinates contain Z values.
  pub(crate) has_z: bool,
  /// Indicates whether coordinates contain M values.
  pub(crate) has_m: bool,
}

/// Collects non-point index inputs before fixed metadata fields are applied.
pub(crate) struct ClusteringIndexXZInput {
  /// Identifies the field containing XZ-order codes.
  pub(crate) code: ColumnPath,
  /// Defines the geometry payload encoding.
  pub(crate) encoding: GeodisplayEncoding,
  /// Defines the Geodisplay geometry category.
  pub(crate) geometry_type: GeometryType,
  /// Defines the indexed dataset extent.
  pub(crate) full_extent: Extent2D,
  /// Limits the XZ hierarchy depth.
  pub(crate) max_level: u32,
  /// Provides the coordinate reference authority code.
  pub(crate) wkid: Option<u32>,
  /// Provides the coordinate reference WKT.
  pub(crate) wkt: Option<String>,
  /// Indicates whether coordinates contain Z values.
  pub(crate) has_z: bool,
  /// Indicates whether coordinates contain M values.
  pub(crate) has_m: bool,
  /// Lists generated multiscale geometry columns.
  pub(crate) levels: Vec<MultiscaleLevelInput>,
}

/// Collects one multiscale level before serialization fields are assembled.
pub(crate) struct MultiscaleLevelInput {
  /// Identifies the generated payload column.
  pub(crate) column: ColumnPath,
  /// Defines the multiscale level.
  pub(crate) level: u16,
  /// Defines the coordinate resolution.
  pub(crate) resolution: f64,
  /// Defines the map scale denominator.
  pub(crate) scale: f64,
  /// Defines per-axis quantization scale values.
  pub(crate) transform_scale: [f64; 4],
  /// Defines per-axis quantization origins.
  pub(crate) transform_translate: [f64; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct MultiscaleLevel {
  pub(crate) column: ColumnPath,
  pub(crate) level: u16,
  pub(crate) resolution: f64,
  pub(crate) scale: f64,
  pub(crate) transform: QuantizationTransform,
}

impl GeodisplayMetadata {
  pub(super) fn point(index: ClusteringIndexZ) -> Self {
    Self::Z { index }
  }

  pub(super) fn xz(index: ClusteringIndexXZ) -> Self {
    Self::Xz { index }
  }
}

impl GeodisplayMetadata {
  /// Serialize this Geodisplay metadata into one Parquet key-value entry.
  pub(crate) fn parquet_entry(&self) -> Result<KeyValue> {
    Ok(KeyValue::new(
      "geodisplay".to_string(),
      Some(serde_json::to_string(self)?),
    ))
  }
}

impl ClusteringIndexZ {
  pub(super) fn new(input: ClusteringIndexZInput) -> Self {
    Self {
      version: GEODISPLAY_VERSION.to_string(),
      writer: Some(sop_writer_metadata()),
      code: input.code,
      wkid: input.wkid,
      wkt: input.wkt,
      x_column: input.x_column,
      y_column: input.y_column,
      z_column: input.z_column,
      m_column: input.m_column,
      coordinate_precision: input.coordinate_precision,
      full_extent: input.full_extent,
      geometry_type: GeometryType::Point,
      has_z: input.has_z,
      has_m: input.has_m,
    }
  }
}

impl ClusteringIndexXZ {
  pub(super) fn new(input: ClusteringIndexXZInput) -> Self {
    Self {
      version: GEODISPLAY_VERSION.to_string(),
      writer: Some(sop_writer_metadata()),
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

fn sop_writer_metadata() -> WriterMetadata {
  WriterMetadata {
    name: SOP_WRITER_NAME.to_string(),
    version: SOP_WRITER_VERSION.to_string(),
  }
}

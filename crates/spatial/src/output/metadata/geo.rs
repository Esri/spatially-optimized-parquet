//! Defines the GeoParquet JSON contract.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::geometry::{Extent2D, GeometryKind};
use crate::output::SpatialReferenceInfo;

use super::parquet::ParquetMetadata;

/// Stores values serialized into one GeoParquet geometry-column contract.
pub(crate) struct GeoMetadataInput<'a> {
  /// Names the primary geometry column.
  pub(crate) geometry_column: &'a str,
  /// Stores exact geometry kinds present in the output.
  pub(crate) geometry_types: &'a [GeometryKind],
  /// Stores the geometry extent in output coordinates.
  pub(crate) output_extent: Extent2D,
  /// Stores the output coordinate reference system.
  pub(crate) output_spatial_reference: &'a SpatialReferenceInfo,
  /// Indicates whether geometry values contain Z ordinates.
  pub(crate) has_z: bool,
  /// Indicates whether geometry values contain M ordinates.
  pub(crate) has_m: bool,
  /// Enables GeoParquet covering metadata.
  pub(crate) covering: bool,
  /// Names the covering struct column.
  pub(crate) covering_column: &'a str,
}

/// Represents the GeoParquet 1.1 file metadata contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct GeoMetadata {
  pub(crate) version: String,
  pub(crate) primary_column: String,
  pub(crate) columns: BTreeMap<String, GeoColumnMetadata>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct GeoColumnMetadata {
  pub(crate) encoding: String,
  pub(crate) geometry_types: Vec<String>,
  pub(crate) bbox: [f64; 4],
  pub(crate) crs: Value,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) covering: Option<GeoCovering>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GeoCovering {
  pub(crate) bbox: GeoCoveringBbox,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct GeoCoveringBbox {
  pub(crate) xmin: Vec<String>,
  pub(crate) ymin: Vec<String>,
  pub(crate) xmax: Vec<String>,
  pub(crate) ymax: Vec<String>,
}

impl GeoMetadata {
  pub(super) fn new(input: GeoMetadataInput<'_>) -> Result<Self> {
    let geometry_types = input
      .geometry_types
      .iter()
      .copied()
      .map(|geometry_kind| geoparquet_geometry_type_name(geometry_kind, input.has_z, input.has_m))
      .collect::<Result<Vec<_>>>()?;
    let column = GeoColumnMetadata {
      encoding: "WKB".to_string(),
      geometry_types,
      bbox: [
        input.output_extent.xmin,
        input.output_extent.ymin,
        input.output_extent.xmax,
        input.output_extent.ymax,
      ],
      crs: input
        .output_spatial_reference
        .projjson
        .clone()
        .context("missing output CRS PROJJSON")?,
      covering: input
        .covering
        .then(|| GeoCovering::new(input.covering_column)),
    };
    Ok(Self {
      version: "1.1.0".to_string(),
      primary_column: input.geometry_column.to_string(),
      columns: BTreeMap::from([(input.geometry_column.to_string(), column)]),
    })
  }
}

impl ParquetMetadata for GeoMetadata {
  const KEY: &'static str = "geo";
}

impl GeoCovering {
  fn new(column: &str) -> Self {
    Self {
      bbox: GeoCoveringBbox {
        xmin: vec![column.to_string(), "xmin".to_string()],
        ymin: vec![column.to_string(), "ymin".to_string()],
        xmax: vec![column.to_string(), "xmax".to_string()],
        ymax: vec![column.to_string(), "ymax".to_string()],
      },
    }
  }
}

fn geoparquet_geometry_type_name(
  geometry_kind: GeometryKind,
  has_z: bool,
  has_m: bool,
) -> Result<String> {
  let base = match geometry_kind {
    GeometryKind::Point => "Point",
    GeometryKind::LineString => "LineString",
    GeometryKind::MultiPoint => "MultiPoint",
    GeometryKind::MultiLineString => "MultiLineString",
    GeometryKind::Polygon => "Polygon",
    GeometryKind::MultiPolygon => "MultiPolygon",
    GeometryKind::GeometryCollection => "GeometryCollection",
    GeometryKind::Unknown => return Err(anyhow::anyhow!("unsupported geometry kind metadata")),
  };
  let suffix = match (has_z, has_m) {
    (false, false) => "",
    (true, false) => " Z",
    (false, true) => " M",
    (true, true) => " ZM",
  };
  Ok(format!("{base}{suffix}"))
}

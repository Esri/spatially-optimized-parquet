// Copyright 2026 Esri
//
// Licensed under the Apache License Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Defines and serializes the GeoParquet JSON metadata contract.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::geometry::{Extent2D, GeometryKind};
use crate::geoparquet::SpatialReference;
use crate::geoparquet::{GeoParquetError, LodMetadata, OrderingMetadata};

use ::parquet::file::metadata::KeyValue;

/// Represents values serialized into one GeoParquet geometry-column contract.
pub(crate) struct GeoMetadataInput<'a> {
  /// Identifies the primary geometry column.
  pub(crate) geometry_column: &'a str,
  /// Defines exact geometry kinds present in the output.
  pub(crate) geometry_types: &'a [GeometryKind],
  /// Defines the geometry extent in output coordinates.
  pub(crate) output_extent: Extent2D,
  /// Provides the output coordinate reference system.
  pub(crate) output_spatial_reference: &'a SpatialReference,
  /// Indicates whether geometry values contain Z components.
  pub(crate) has_z: bool,
  /// Indicates whether geometry values contain M components.
  pub(crate) has_m: bool,
  /// Enables GeoParquet covering metadata.
  pub(crate) covering: bool,
  /// Identifies the covering struct column.
  pub(crate) covering_column: &'a str,
  /// Adds draft spatial ordering metadata when requested.
  pub(crate) ordering: Option<OrderingMetadata>,
  /// Adds draft level-of-detail metadata when requested.
  pub(crate) lod: Option<LodMetadata>,
}

/// Represents the GeoParquet 2.0 file metadata contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct GeoMetadata {
  pub(crate) version: String,
  pub(crate) primary_column: String,
  pub(crate) columns: BTreeMap<String, GeoColumnMetadata>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) ordering: Option<OrderingMetadata>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) lod: Option<LodMetadata>,
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
  pub(super) fn new(input: GeoMetadataInput<'_>) -> Result<Self, GeoParquetError> {
    let geometry_types = input
      .geometry_types
      .iter()
      .copied()
      .map(|geometry_kind| geometry_kind.geoparquet_type_name(input.has_z, input.has_m))
      .collect::<Result<Vec<_>, GeoParquetError>>()?;
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
        .ok_or_else(|| {
          GeoParquetError::Metadata("missing output spatial-reference PROJJSON".to_string())
        })?,
      covering: input
        .covering
        .then(|| GeoCovering::new(input.covering_column)),
    };
    Ok(Self {
      version: "2.0.0".to_string(),
      primary_column: input.geometry_column.to_string(),
      columns: BTreeMap::from([(input.geometry_column.to_string(), column)]),
      ordering: input.ordering,
      lod: input.lod,
    })
  }

  /// Serialize GeoParquet metadata while preserving non-reserved source entries.
  pub(crate) fn parquet_entries(
    mut source_entries: Vec<KeyValue>,
    input: GeoMetadataInput<'_>,
  ) -> Result<Vec<KeyValue>, GeoParquetError> {
    source_entries.retain(|entry| entry.key != "geo");
    source_entries.push(Self::parquet_entry(input)?);
    Ok(source_entries)
  }

  /// Serialize one GeoParquet metadata entry.
  pub(crate) fn parquet_entry(input: GeoMetadataInput<'_>) -> Result<KeyValue, GeoParquetError> {
    Ok(KeyValue::new(
      "geo".to_string(),
      Some(
        serde_json::to_string(&Self::new(input)?).map_err(|source| GeoParquetError::Json {
          operation: "serialize GeoParquet metadata",
          source,
        })?,
      ),
    ))
  }
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

impl GeometryKind {
  fn geoparquet_type_name(self, has_z: bool, has_m: bool) -> Result<String, GeoParquetError> {
    let base = match self {
      Self::Point => "Point",
      Self::LineString => "LineString",
      Self::MultiPoint => "MultiPoint",
      Self::MultiLineString => "MultiLineString",
      Self::Polygon => "Polygon",
      Self::MultiPolygon => "MultiPolygon",
      Self::GeometryCollection => "GeometryCollection",
      Self::Unknown => {
        return Err(GeoParquetError::Metadata(
          "unsupported geometry kind metadata".to_string(),
        ));
      }
    };
    let suffix = match (has_z, has_m) {
      (false, false) => "",
      (true, false) => " Z",
      (false, true) => " M",
      (true, true) => " ZM",
    };
    Ok(format!("{base}{suffix}"))
  }
}

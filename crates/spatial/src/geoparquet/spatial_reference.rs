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

//! Resolves source spatial-reference metadata and defines supported GeoParquet output references.

use gdal::spatial_ref::{AxisMappingStrategy, SpatialRef};
use serde_json::Value;

use super::GeoParquetError;
use crate::input::SourceGeometryMetadata;

/// Selects the default output spatial reference.
pub const DEFAULT_OUTPUT_WKID: u32 = 4326;
/// Selects the supported projected output spatial reference.
pub(crate) const WEB_MERCATOR_OUTPUT_WKID: u32 = 3857;
/// Defines the maximum absolute Web Mercator coordinate in meters.
pub(crate) const WEB_MERCATOR_MAX_COORDINATE: f64 = 20_037_508.342_789_244;
/// Defines the width of the canonical Web Mercator square in meters.
pub(crate) const WEB_MERCATOR_WORLD_WIDTH: f64 = WEB_MERCATOR_MAX_COORDINATE * 2.0;

#[derive(Debug, Clone, PartialEq, Default)]
/// Represents equivalent identifiers and definitions for one coordinate reference system.
pub(crate) struct SpatialReference {
  /// Provides an inferred EPSG well-known identifier when available.
  pub(crate) wkid: Option<u32>,
  /// Provides a WKT definition when available.
  pub(crate) wkt: Option<String>,
  /// Provides the authoritative PROJJSON definition.
  pub(crate) projjson: Option<Value>,
}

impl SpatialReference {
  /// Validate the requested output spatial reference before opening job resources.
  pub(crate) fn validate_output_wkid(output_wkid: u32) -> Result<(), GeoParquetError> {
    match output_wkid {
      DEFAULT_OUTPUT_WKID | WEB_MERCATOR_OUTPUT_WKID => Ok(()),
      wkid => Err(GeoParquetError::UnsupportedOutputSpatialReference { wkid }),
    }
  }

  /// Resolve one spatial reference from source metadata or an input WKID override.
  pub(crate) fn try_new(
    source_geometry: Option<&SourceGeometryMetadata>,
    geometry_column: &str,
    input_wkid: Option<u32>,
  ) -> Result<Self, GeoParquetError> {
    match (
      input_wkid,
      source_geometry.and_then(|geometry| geometry.projjson.as_ref()),
    ) {
      (Some(_), Some(_)) => Err(GeoParquetError::Metadata(format!(
        "--in-sr cannot be used because geometry column '{geometry_column}' already has spatial-reference metadata"
      ))),
      (Some(wkid), None) => Self::from_epsg(wkid),
      (None, Some(projjson)) => Self::from_projjson(projjson),
      (None, None) => Err(GeoParquetError::Metadata(format!(
        "missing spatial-reference metadata for geometry column '{geometry_column}'; \
         pass --in-sr <LATEST_WKID>"
      ))),
    }
  }

  /// Construct spatial-reference metadata from an EPSG well-known identifier.
  pub(crate) fn from_epsg(wkid: u32) -> Result<Self, GeoParquetError> {
    let mut spatial_ref =
      SpatialRef::from_epsg(wkid).map_err(|source| GeoParquetError::SpatialReference {
        operation: "load EPSG definition",
        source,
      })?;
    spatial_ref.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    let projjson =
      spatial_ref
        .to_projjson()
        .map_err(|source| GeoParquetError::SpatialReference {
          operation: "export EPSG definition as PROJJSON",
          source,
        })?;
    let projjson = serde_json::from_str(&projjson).map_err(|source| GeoParquetError::Json {
      operation: "decode spatial-reference PROJJSON",
      source,
    })?;
    Self::from_projjson(&projjson)
  }

  /// Construct spatial-reference metadata from an authoritative PROJJSON definition.
  pub(crate) fn from_projjson(projjson: &Value) -> Result<Self, GeoParquetError> {
    let spatial_ref = Self::spatial_ref_from_projjson(projjson)?;
    Ok(Self {
      wkid: projjson.get("id").and_then(supported_authority_code),
      wkt: spatial_ref.to_wkt().ok(),
      projjson: Some(projjson.clone()),
    })
  }

  /// Serialize the authoritative PROJJSON definition for deferred transformation.
  pub(crate) fn definition(&self) -> Result<String, GeoParquetError> {
    let projjson = self.projjson()?;
    serde_json::to_string(projjson).map_err(|source| GeoParquetError::Json {
      operation: "serialize spatial-reference PROJJSON",
      source,
    })
  }

  /// Return the authoritative PROJJSON definition.
  pub(crate) fn projjson(&self) -> Result<&Value, GeoParquetError> {
    self.projjson.as_ref().ok_or_else(|| {
      GeoParquetError::Metadata("missing spatial-reference PROJJSON definition".to_string())
    })
  }

  /// Construct a GDAL spatial reference with traditional GIS axis order.
  pub(crate) fn spatial_ref(&self) -> Result<SpatialRef, GeoParquetError> {
    let definition = self.definition()?;
    Self::spatial_ref_from_definition(&definition)
  }

  /// Construct a GDAL spatial reference from a PROJJSON definition.
  pub(crate) fn spatial_ref_from_definition(
    definition: &str,
  ) -> Result<SpatialRef, GeoParquetError> {
    let mut spatial_ref = SpatialRef::from_definition(definition).map_err(|source| {
      GeoParquetError::SpatialReference {
        operation: "load spatial reference",
        source,
      }
    })?;
    spatial_ref.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    Ok(spatial_ref)
  }

  fn spatial_ref_from_projjson(projjson: &Value) -> Result<SpatialRef, GeoParquetError> {
    let definition = serde_json::to_string(projjson).map_err(|source| GeoParquetError::Json {
      operation: "serialize spatial-reference PROJJSON",
      source,
    })?;
    Self::spatial_ref_from_definition(&definition)
  }
}

fn supported_authority_code(value: &Value) -> Option<u32> {
  let authority = value.get("authority").and_then(Value::as_str);
  let code = value.get("code").and_then(|value| {
    value
      .as_u64()
      .and_then(|value| u32::try_from(value).ok())
      .or_else(|| value.as_str().and_then(|value| value.parse::<u32>().ok()))
  });
  matches!(authority, Some("EPSG" | "ESRI"))
    .then_some(code)
    .flatten()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn supported_output_wkids_are_accepted() {
    for output_wkid in [DEFAULT_OUTPUT_WKID, WEB_MERCATOR_OUTPUT_WKID] {
      SpatialReference::validate_output_wkid(output_wkid).unwrap();
    }
  }

  #[test]
  fn unsupported_output_wkid_returns_error() {
    let error = SpatialReference::validate_output_wkid(4269).unwrap_err();
    assert!(matches!(
      error,
      GeoParquetError::UnsupportedOutputSpatialReference { wkid: 4269 }
    ));
  }
}

//! Resolves source spatial-reference metadata and defines supported GeoParquet output references.

use anyhow::{Context, Result, bail};
use gdal::spatial_ref::{AxisMappingStrategy, SpatialRef};
use serde_json::Value;

use crate::input::SourceGeometryMetadata;

/// Selects the default output spatial reference.
pub const DEFAULT_OUTPUT_WKID: u32 = 4326;
/// Selects the projected spatial reference supported by optimized output.
pub(crate) const WEB_MERCATOR_OUTPUT_WKID: u32 = 3857;

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
  pub(crate) fn validate_output_wkid(output_wkid: u32) {
    if output_wkid != DEFAULT_OUTPUT_WKID {
      todo!("output spatial reference EPSG:{output_wkid}");
    }
  }

  /// Resolve one spatial reference from source metadata or an input WKID override.
  pub(crate) fn try_new(
    source_geometry: Option<&SourceGeometryMetadata>,
    geometry_column: &str,
    input_wkid: Option<u32>,
  ) -> Result<Self> {
    match (
      input_wkid,
      source_geometry.and_then(|geometry| geometry.projjson.as_ref()),
    ) {
      (Some(_), Some(_)) => bail!(
        "--in-sr cannot be used because geometry column '{geometry_column}' already has spatial-reference metadata"
      ),
      (Some(wkid), None) => Self::from_epsg(wkid),
      (None, Some(projjson)) => Self::from_projjson(projjson),
      (None, None) => bail!(
        "missing spatial-reference metadata for geometry column '{geometry_column}'; \
         pass --in-sr <LATEST_WKID>"
      ),
    }
  }

  /// Construct spatial-reference metadata from an EPSG well-known identifier.
  pub(crate) fn from_epsg(wkid: u32) -> Result<Self> {
    let mut spatial_ref =
      SpatialRef::from_epsg(wkid).with_context(|| format!("load EPSG:{wkid}"))?;
    spatial_ref.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    let projjson = spatial_ref
      .to_projjson()
      .with_context(|| format!("export EPSG:{wkid} as PROJJSON"))?;
    let projjson = serde_json::from_str(&projjson).context("decode spatial-reference PROJJSON")?;
    Self::from_projjson(&projjson)
  }

  /// Construct spatial-reference metadata from an authoritative PROJJSON definition.
  pub(crate) fn from_projjson(projjson: &Value) -> Result<Self> {
    let spatial_ref = Self::spatial_ref_from_projjson(projjson)?;
    Ok(Self {
      wkid: projjson.get("id").and_then(supported_authority_code),
      wkt: spatial_ref.to_wkt().ok(),
      projjson: Some(projjson.clone()),
    })
  }

  /// Serialize the authoritative PROJJSON definition for deferred transformation.
  pub(crate) fn definition(&self) -> Result<String> {
    let projjson = self.projjson()?;
    serde_json::to_string(projjson).context("serialize spatial reference as PROJJSON")
  }

  /// Return the authoritative PROJJSON definition.
  pub(crate) fn projjson(&self) -> Result<&Value> {
    self
      .projjson
      .as_ref()
      .context("missing spatial-reference PROJJSON definition")
  }

  /// Construct a GDAL spatial reference with traditional GIS axis order.
  pub(crate) fn spatial_ref(&self) -> Result<SpatialRef> {
    let definition = self.definition()?;
    Self::spatial_ref_from_definition(&definition)
  }

  /// Construct a GDAL spatial reference from a PROJJSON definition.
  pub(crate) fn spatial_ref_from_definition(definition: &str) -> Result<SpatialRef> {
    let mut spatial_ref =
      SpatialRef::from_definition(definition).context("load spatial reference")?;
    spatial_ref.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
    Ok(spatial_ref)
  }

  fn spatial_ref_from_projjson(projjson: &Value) -> Result<SpatialRef> {
    let definition =
      serde_json::to_string(projjson).context("serialize spatial-reference PROJJSON")?;
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

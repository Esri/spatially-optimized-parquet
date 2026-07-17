//! Resolves source spatial-reference metadata and defines supported GeoParquet output references.

use anyhow::{Context, Result, bail};
use gdal::spatial_ref::{AxisMappingStrategy, SpatialRef};
use serde_json::Value;

use crate::geometry::GeometryEncoding;
use crate::input::{SourceDatasetMetadata, SourceGeometryMetadata};

/// Selects the default output spatial reference.
pub const DEFAULT_OUTPUT_WKID: u32 = 4326;
/// Selects the projected spatial reference supported by optimized output.
pub(crate) const WEB_MERCATOR_OUTPUT_WKID: u32 = 3857;

#[derive(Debug, Clone, PartialEq, Default)]
/// Stores equivalent identifiers and definitions for one coordinate reference system.
pub(crate) struct SpatialReference {
  /// Stores an EPSG well-known identifier when one can be inferred.
  pub(crate) wkid: Option<u32>,
  /// Stores a WKT definition when available.
  pub(crate) wkt: Option<String>,
  /// Stores the authoritative PROJJSON definition.
  pub(crate) projjson: Option<Value>,
}

impl SpatialReference {
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
    let projjson = self
      .projjson
      .as_ref()
      .context("missing spatial-reference PROJJSON definition")?;
    serde_json::to_string(projjson).context("serialize spatial reference as PROJJSON")
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

/// Validate the requested output spatial reference before opening job resources.
pub(crate) fn validate_output_wkid(output_wkid: u32) {
  if output_wkid != DEFAULT_OUTPUT_WKID {
    todo!("output spatial reference EPSG:{output_wkid}");
  }
}

pub(super) fn resolve_source_spatial_reference(
  source_metadata: &mut SourceDatasetMetadata,
  geometry_column: &str,
  input_wkid: Option<u32>,
) -> Result<()> {
  let existing_geometry = source_metadata
    .geometry
    .as_ref()
    .filter(|geometry| geometry.column == geometry_column);
  if let Some(input_wkid) = input_wkid {
    if existing_geometry
      .and_then(|geometry| geometry.projjson.as_ref())
      .is_some()
    {
      bail!(
        "--in-sr cannot be used because geometry column '{geometry_column}' already has spatial-reference metadata"
      );
    }
    let projjson = SpatialReference::from_epsg(input_wkid)?
      .projjson
      .context("EPSG spatial reference did not produce PROJJSON")?;
    let existing = existing_geometry.cloned();
    source_metadata.geometry = Some(SourceGeometryMetadata {
      column: geometry_column.to_string(),
      encoding: GeometryEncoding::Wkb,
      geometry_types: existing
        .as_ref()
        .map(|geometry| geometry.geometry_types.clone())
        .unwrap_or_default(),
      bbox: existing.as_ref().and_then(|geometry| geometry.bbox),
      covering: existing
        .as_ref()
        .and_then(|geometry| geometry.covering.clone()),
      projjson: Some(projjson),
      has_z: existing.as_ref().is_some_and(|geometry| geometry.has_z),
      has_m: existing.as_ref().is_some_and(|geometry| geometry.has_m),
    });
  } else if existing_geometry
    .and_then(|geometry| geometry.projjson.as_ref())
    .is_none()
  {
    bail!(
      "missing spatial-reference metadata for geometry column '{geometry_column}'; \
       pass --in-sr <LATEST_WKID>"
    );
  }
  Ok(())
}

fn supported_authority_code(value: &Value) -> Option<u32> {
  let authority = value.get("authority").and_then(Value::as_str);
  let code = value.get("code").and_then(value_as_u32);
  matches!(authority, Some("EPSG" | "ESRI"))
    .then_some(code)
    .flatten()
}

fn value_as_u32(value: &Value) -> Option<u32> {
  value
    .as_u64()
    .and_then(|value| u32::try_from(value).ok())
    .or_else(|| value.as_str().and_then(|value| value.parse::<u32>().ok()))
}

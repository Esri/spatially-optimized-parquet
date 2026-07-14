//! Resolves source CRS overrides and metadata into stable spatial-reference state.

use anyhow::{Context, Result, bail};
use gdal::spatial_ref::{AxisMappingStrategy, SpatialRef};
use serde_json::Value;

use crate::geometry::GeometryEncoding;
use crate::input::{SourceDatasetMetadata, SourceGeometryMetadata};
use crate::output::SpatialReferenceInfo;

pub(super) fn apply_input_wkid(
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
        "--in-sr cannot be used because geometry column '{geometry_column}' already has CRS metadata"
      );
    }
    let projjson = projjson_from_epsg(input_wkid)?;
    let existing = existing_geometry.cloned();
    source_metadata.geometry = Some(SourceGeometryMetadata {
      column: geometry_column.to_string(),
      encoding: GeometryEncoding::Wkb,
      geometry_types: existing
        .as_ref()
        .map(|geometry| geometry.geometry_types.clone())
        .unwrap_or_default(),
      bbox: existing.as_ref().and_then(|geometry| geometry.bbox),
      projjson: Some(projjson),
      has_z: existing.as_ref().is_some_and(|geometry| geometry.has_z),
      has_m: existing.as_ref().is_some_and(|geometry| geometry.has_m),
    });
  } else if existing_geometry
    .and_then(|geometry| geometry.projjson.as_ref())
    .is_none()
  {
    bail!(
      "missing CRS metadata for geometry column '{geometry_column}'; pass --in-sr <LATEST_WKID>"
    );
  }
  Ok(())
}

fn projjson_from_epsg(wkid: u32) -> Result<Value> {
  let mut spatial_ref =
    SpatialRef::from_epsg(wkid).with_context(|| format!("load input EPSG:{wkid}"))?;
  spatial_ref.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
  let projjson = spatial_ref
    .to_projjson()
    .with_context(|| format!("export input EPSG:{wkid} as PROJJSON"))?;
  serde_json::from_str(&projjson).context("decode input CRS PROJJSON")
}

pub(super) fn spatial_reference_info(projjson: &Value) -> Result<SpatialReferenceInfo> {
  let definition = serde_json::to_string(projjson).context("serialize input CRS PROJJSON")?;
  let mut spatial_ref =
    SpatialRef::from_definition(&definition).context("load input spatial reference")?;
  spatial_ref.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
  Ok(SpatialReferenceInfo {
    wkid: projjson.get("id").and_then(supported_authority_code),
    wkt: spatial_ref.to_wkt().ok(),
    projjson: Some(projjson.clone()),
  })
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

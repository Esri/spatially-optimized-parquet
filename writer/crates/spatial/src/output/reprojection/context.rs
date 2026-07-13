//! Resolves target coordinate-reference metadata and deferred transform definitions.

use anyhow::{Context, Result};
use gdal::spatial_ref::{AxisMappingStrategy, SpatialRef};
use serde_json::Value;

use crate::analysis::SpatialReferenceInfo;

use super::PreparedTransform;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Stores source and target CRS definitions for deferred transform construction.
pub struct TransformSpec {
  source_definition: String,
  target_definition: String,
}

#[derive(Debug, Clone)]
/// Stores target CRS metadata and the optional transform required to produce it.
pub struct ReprojectionContext {
  transform: Option<TransformSpec>,
  target_spatial_reference: SpatialReferenceInfo,
}

impl ReprojectionContext {
  /// Resolve target CRS metadata and transformation from source PROJJSON.
  pub fn from_source_projjson(source_projjson: &Value, target_wkid: u32) -> Result<Self> {
    let source_definition =
      serde_json::to_string(source_projjson).context("serialize source CRS as PROJJSON")?;
    let source_spatial_ref = spatial_ref_from_definition(&source_definition)?;
    let target_spatial_ref = spatial_ref_from_epsg(target_wkid)?;
    let target_definition = target_spatial_ref
      .to_projjson()
      .context("export target CRS as PROJJSON")?;

    Ok(Self {
      transform: (source_spatial_ref != target_spatial_ref).then_some(TransformSpec {
        source_definition,
        target_definition: target_definition.clone(),
      }),
      target_spatial_reference: SpatialReferenceInfo {
        wkid: Some(target_wkid),
        wkt: Some(
          target_spatial_ref
            .to_wkt()
            .context("export target CRS as WKT")?,
        ),
        projjson: Some(
          serde_json::from_str(&target_definition).context("decode target CRS PROJJSON")?,
        ),
      },
    })
  }

  /// Return whether source and target spatial references differ.
  pub fn requires_reprojection(&self) -> bool {
    self.transform.is_some()
  }

  /// Return the deferred transform when reprojection is required.
  pub fn transform(&self) -> Option<&TransformSpec> {
    self.transform.as_ref()
  }

  /// Return metadata describing the target coordinate reference system.
  pub fn target_spatial_reference(&self) -> &SpatialReferenceInfo {
    &self.target_spatial_reference
  }
}

impl TransformSpec {
  /// Build reusable transformation state from the stored CRS definitions.
  pub fn prepare(&self) -> Result<PreparedTransform> {
    PreparedTransform::new(
      spatial_ref_from_definition(&self.source_definition)?,
      spatial_ref_from_definition(&self.target_definition)?,
    )
  }
}

fn spatial_ref_from_epsg(wkid: u32) -> Result<SpatialRef> {
  let mut spatial_ref =
    SpatialRef::from_epsg(wkid).with_context(|| format!("load target EPSG:{wkid}"))?;
  spatial_ref.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
  Ok(spatial_ref)
}

fn spatial_ref_from_definition(definition: &str) -> Result<SpatialRef> {
  let mut spatial_ref =
    SpatialRef::from_definition(definition).context("load spatial reference definition")?;
  spatial_ref.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
  Ok(spatial_ref)
}

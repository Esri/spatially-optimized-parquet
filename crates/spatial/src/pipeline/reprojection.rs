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

//! Resolves output coordinate systems and applies deferred GeoParquet geometry reprojection.
//!
//! Builds target spatial-reference metadata, retains a serializable source-to-target
//! transformation, prepares GDAL transformation state during execution, and exposes a DataFusion
//! expression that rewrites WKB geometry lazily.

use arrow_array::ArrayRef;
use arrow_array::builder::BinaryBuilder;
use arrow_schema::DataType;
use datafusion::common::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::{
  ColumnarValue, Expr, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature,
};
use datafusion::prelude::col;
use gdal::spatial_ref::{CoordTransform, SpatialRef};
use gdal::vector::Geometry;
use serde_json::Value;
use std::sync::Arc;

use crate::geometry::{GeometryArray, GeometryError, geometry_signature, to_datafusion_error};
use crate::geoparquet::{SpatialReference, WEB_MERCATOR_MAX_COORDINATE, WEB_MERCATOR_OUTPUT_WKID};
use crate::pipeline::PipelineError;

#[derive(Debug, Clone)]
/// Represents target spatial-reference metadata and the source definition for deferred reprojection.
pub(crate) struct ResolvedReprojection {
  source_definition: Option<String>,
  target_spatial_reference: SpatialReference,
}

impl ResolvedReprojection {
  /// Resolve target spatial-reference metadata and transformation from source PROJJSON.
  pub(crate) fn from_source_projjson(
    source_projjson: &Value,
    target_wkid: u32,
  ) -> Result<Self, PipelineError> {
    let source_spatial_reference = SpatialReference::from_projjson(source_projjson)?;
    let target_spatial_reference = SpatialReference::from_epsg(target_wkid)?;
    let source_definition = source_spatial_reference.definition()?;

    Ok(Self {
      source_definition: (source_spatial_reference.spatial_ref()?
        != target_spatial_reference.spatial_ref()?)
      .then_some(source_definition),
      target_spatial_reference,
    })
  }

  /// Return whether source and target spatial references differ.
  pub(crate) fn requires_reprojection(&self) -> bool {
    self.source_definition.is_some()
  }

  /// Return metadata describing the target coordinate reference system.
  pub(crate) fn target_spatial_reference(&self) -> &SpatialReference {
    &self.target_spatial_reference
  }

  /// Build output geometry normalization when transformation or domain validation is required.
  pub(crate) fn output_geometry_expr(
    &self,
    geometry_column: &str,
  ) -> Result<Option<Expr>, PipelineError> {
    let target_wkid = self.target_spatial_reference.wkid.ok_or_else(|| {
      PipelineError::InvalidRequest("missing output spatial-reference WKID".to_string())
    })?;
    if self.source_definition.is_none() && target_wkid != WEB_MERCATOR_OUTPUT_WKID {
      return Ok(None);
    }
    let target_definition = self.target_spatial_reference.definition()?;
    Ok(Some(
      OutputGeometryUdf::scalar_udf(
        self.source_definition.clone(),
        target_definition,
        target_wkid,
      )
      .call(vec![col(geometry_column)]),
    ))
  }
}

#[derive(Debug)]
/// Owns a prepared GDAL coordinate transform and the spatial references backing it.
struct PreparedTransform {
  _source: SpatialRef,
  _target: SpatialRef,
  coord_transform: CoordTransform,
}

impl PreparedTransform {
  /// Construct a coordinate operation backed by source and target spatial references.
  fn new(source: SpatialRef, target: SpatialRef) -> Result<Self, GeometryError> {
    let coord_transform = CoordTransform::new(&source, &target).map_err(|error| {
      GeometryError::InvalidGeometry(format!("create coordinate transform: {error}"))
    })?;
    Ok(Self {
      _source: source,
      _target: target,
      coord_transform,
    })
  }

  /// Reproject one geometry into the target spatial reference.
  fn reproject_geometry(&self, geometry: Geometry) -> Result<Geometry, GeometryError> {
    geometry
      .transform(&self.coord_transform)
      .map_err(|error| GeometryError::InvalidGeometry(format!("reproject geometry: {error}")))
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct OutputGeometryUdf {
  source_definition: Option<String>,
  target_definition: String,
  target_wkid: u32,
}

impl OutputGeometryUdf {
  fn scalar_udf(
    source_definition: Option<String>,
    target_definition: String,
    target_wkid: u32,
  ) -> ScalarUDF {
    ScalarUDF::new_from_impl(Self {
      source_definition,
      target_definition,
      target_wkid,
    })
  }

  fn normalize_wkb(
    &self,
    bytes: &[u8],
    prepared: Option<&PreparedTransform>,
  ) -> Result<Vec<u8>, GeometryError> {
    let geometry = Geometry::from_wkb(bytes).map_err(|error| {
      GeometryError::InvalidGeometry(format!("decode geometry for output normalization: {error}"))
    })?;
    let geometry = match prepared {
      Some(prepared) => prepared.reproject_geometry(geometry)?,
      None => geometry,
    };
    if self.target_wkid == WEB_MERCATOR_OUTPUT_WKID {
      validate_web_mercator_geometry(&geometry)?;
    }
    geometry.wkb().map_err(|error| {
      GeometryError::InvalidGeometry(format!("encode normalized geometry as WKB: {error}"))
    })
  }
}

impl ScalarUDFImpl for OutputGeometryUdf {
  fn name(&self) -> &str {
    "output_geometry"
  }

  fn signature(&self) -> &Signature {
    geometry_signature()
  }

  fn return_type(&self, _: &[DataType]) -> DataFusionResult<DataType> {
    Ok(DataType::Binary)
  }

  fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DataFusionResult<ColumnarValue> {
    let arrays = ColumnarValue::values_to_arrays(&args.args)?;
    let geometry = arrays
      .first()
      .ok_or_else(|| DataFusionError::Execution("missing geometry argument".to_string()))?;
    let prepared = self
      .source_definition
      .as_ref()
      .map(|source_definition| {
        PreparedTransform::new(
          SpatialReference::spatial_ref_from_definition(source_definition)
            .map_err(to_datafusion_error)?,
          SpatialReference::spatial_ref_from_definition(&self.target_definition)
            .map_err(to_datafusion_error)?,
        )
        .map_err(to_datafusion_error)
      })
      .transpose()?;
    let geometry = GeometryArray::try_new(geometry.as_ref())?;
    let mut builder = BinaryBuilder::with_capacity(geometry.len(), geometry.len() * 16);
    for value in geometry.values() {
      match value {
        Some(bytes) => builder.append_value(
          self
            .normalize_wkb(bytes, prepared.as_ref())
            .map_err(to_datafusion_error)?,
        ),
        None => builder.append_null(),
      }
    }
    Ok(ColumnarValue::Array(Arc::new(builder.finish()) as ArrayRef))
  }
}

fn validate_web_mercator_geometry(geometry: &Geometry) -> Result<(), GeometryError> {
  let envelope = geometry.envelope();
  let coordinates = [envelope.MinX, envelope.MinY, envelope.MaxX, envelope.MaxY];
  if coordinates
    .iter()
    .any(|value| !value.is_finite() || value.abs() > WEB_MERCATOR_MAX_COORDINATE)
  {
    return Err(GeometryError::InvalidGeometry(format!(
      "Web Mercator geometry exceeds canonical EPSG:3857 bounds of ±{WEB_MERCATOR_MAX_COORDINATE} meters"
    )));
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn web_mercator_geometry_accepts_canonical_bounds() {
    let boundary = WEB_MERCATOR_MAX_COORDINATE;
    let geometry = Geometry::from_wkt(&format!(
      "LINESTRING (-{boundary} -{boundary}, {boundary} {boundary})"
    ))
    .unwrap();
    validate_web_mercator_geometry(&geometry).unwrap();
  }

  #[test]
  fn web_mercator_geometry_rejects_coordinates_outside_canonical_bounds() {
    let outside = WEB_MERCATOR_MAX_COORDINATE + 1.0;
    let geometry = Geometry::from_wkt(&format!("POINT ({outside} 0)")).unwrap();
    let error = validate_web_mercator_geometry(&geometry).unwrap_err();
    assert!(error.to_string().contains("canonical EPSG:3857 bounds"));
  }
}

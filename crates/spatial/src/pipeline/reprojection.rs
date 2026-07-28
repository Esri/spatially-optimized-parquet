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
use crate::geoparquet::SpatialReference;
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

  pub(crate) fn geometry_expr(&self, geometry_column: &str) -> Result<Option<Expr>, PipelineError> {
    let Some(source_definition) = self.source_definition.as_ref() else {
      return Ok(None);
    };
    let target_definition = self.target_spatial_reference.definition()?;
    Ok(Some(
      ReprojectGeometryUdf::scalar_udf(source_definition.clone(), target_definition)
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

  /// Reproject one WKB geometry and return target-spatial-reference WKB.
  fn reproject_wkb(&self, bytes: &[u8]) -> Result<Vec<u8>, GeometryError> {
    let geometry = Geometry::from_wkb(bytes).map_err(|error| {
      GeometryError::InvalidGeometry(format!("decode geometry for reprojection: {error}"))
    })?;
    let geometry = geometry
      .transform(&self.coord_transform)
      .map_err(|error| GeometryError::InvalidGeometry(format!("reproject geometry: {error}")))?;
    geometry.wkb().map_err(|error| {
      GeometryError::InvalidGeometry(format!("encode reprojected geometry as WKB: {error}"))
    })
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ReprojectGeometryUdf {
  source_definition: String,
  target_definition: String,
}

impl ReprojectGeometryUdf {
  fn scalar_udf(source_definition: String, target_definition: String) -> ScalarUDF {
    ScalarUDF::new_from_impl(Self {
      source_definition,
      target_definition,
    })
  }
}

impl ScalarUDFImpl for ReprojectGeometryUdf {
  fn name(&self) -> &str {
    "reprojection_geometry"
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
    let prepared = PreparedTransform::new(
      SpatialReference::spatial_ref_from_definition(&self.source_definition)
        .map_err(to_datafusion_error)?,
      SpatialReference::spatial_ref_from_definition(&self.target_definition)
        .map_err(to_datafusion_error)?,
    )
    .map_err(to_datafusion_error)?;
    let geometry = GeometryArray::try_new(geometry.as_ref())?;
    let mut builder = BinaryBuilder::with_capacity(geometry.len(), geometry.len() * 16);
    for value in geometry.values() {
      match value {
        Some(bytes) => {
          builder.append_value(prepared.reproject_wkb(bytes).map_err(to_datafusion_error)?)
        }
        None => builder.append_null(),
      }
    }
    Ok(ColumnarValue::Array(Arc::new(builder.finish()) as ArrayRef))
  }
}

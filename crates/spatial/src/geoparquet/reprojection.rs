//! Resolves output coordinate systems and applies deferred GeoParquet geometry reprojection.
//!
//! Builds target CRS metadata, retains a serializable source-to-target transformation, prepares
//! GDAL transformation state during execution, and exposes a DataFusion expression that rewrites
//! WKB geometry lazily.

use anyhow::{Context, Result};
use arrow_array::ArrayRef;
use arrow_array::builder::BinaryBuilder;
use arrow_schema::DataType;
use datafusion::common::{DataFusionError, Result as DataFusionResult};
use datafusion::logical_expr::{
  ColumnarValue, Expr, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature,
};
use datafusion::prelude::col;
use gdal::spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef};
use gdal::vector::Geometry;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;

use crate::geometry::{GeometryArray, geometry_signature, to_datafusion_error};
use crate::output::SpatialReferenceInfo;

#[derive(Debug, Clone)]
/// Stores target CRS metadata and the source definition required for deferred reprojection.
pub(crate) struct ResolvedReprojection {
  source_definition: Option<String>,
  target_spatial_reference: SpatialReferenceInfo,
}

impl ResolvedReprojection {
  /// Resolve target CRS metadata and transformation from source PROJJSON.
  pub(crate) fn from_source_projjson(source_projjson: &Value, target_wkid: u32) -> Result<Self> {
    let source_definition =
      serde_json::to_string(source_projjson).context("serialize source CRS as PROJJSON")?;
    let source_spatial_ref = spatial_ref_from_definition(&source_definition)?;
    let target_spatial_ref = spatial_ref_from_epsg(target_wkid)?;
    let target_definition = target_spatial_ref
      .to_projjson()
      .context("export target CRS as PROJJSON")?;

    Ok(Self {
      source_definition: (source_spatial_ref != target_spatial_ref).then_some(source_definition),
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
  pub(crate) fn requires_reprojection(&self) -> bool {
    self.source_definition.is_some()
  }

  /// Return metadata describing the target coordinate reference system.
  pub(crate) fn target_spatial_reference(&self) -> &SpatialReferenceInfo {
    &self.target_spatial_reference
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
  fn new(source: SpatialRef, target: SpatialRef) -> Result<Self> {
    let coord_transform =
      CoordTransform::new(&source, &target).context("create coordinate transform")?;
    Ok(Self {
      _source: source,
      _target: target,
      coord_transform,
    })
  }

  /// Reproject one WKB geometry and return target-CRS WKB.
  fn reproject_wkb(&self, bytes: &[u8]) -> Result<Vec<u8>> {
    let geometry = Geometry::from_wkb(bytes).context("decode geometry for reprojection")?;
    let geometry = geometry
      .transform(&self.coord_transform)
      .context("reproject geometry")?;
    geometry.wkb().context("encode reprojected geometry as WKB")
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ReprojectGeometryUdf {
  source_definition: String,
  target_definition: String,
}

impl ScalarUDFImpl for ReprojectGeometryUdf {
  fn as_any(&self) -> &dyn Any {
    self
  }

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
      spatial_ref_from_definition(&self.source_definition).map_err(to_datafusion_error)?,
      spatial_ref_from_definition(&self.target_definition).map_err(to_datafusion_error)?,
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

fn reproject_geometry_udf(source_definition: String, target_definition: String) -> ScalarUDF {
  ScalarUDF::new_from_impl(ReprojectGeometryUdf {
    source_definition,
    target_definition,
  })
}

pub(crate) fn reproject_geometry_expr(
  geometry_column: &str,
  reprojection: &ResolvedReprojection,
) -> Result<Option<Expr>> {
  let Some(source_definition) = reprojection.source_definition.as_ref() else {
    return Ok(None);
  };
  let target_definition = serde_json::to_string(
    reprojection
      .target_spatial_reference
      .projjson
      .as_ref()
      .context("missing target CRS PROJJSON")?,
  )
  .context("serialize target CRS as PROJJSON")?;
  Ok(Some(
    reproject_geometry_udf(source_definition.clone(), target_definition)
      .call(vec![col(geometry_column)]),
  ))
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

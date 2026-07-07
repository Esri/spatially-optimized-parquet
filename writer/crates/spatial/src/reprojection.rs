use anyhow::{Context, Result};
use gdal::spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef};
use gdal::vector::Geometry;
use serde_json::Value;

use crate::analysis::{DisplayGeometryType, Extent2D, SpatialReferenceInfo};
use crate::metadata::source::SourceDatasetMetadata;

const TRANSFORM_BOUNDS_DENSIFY_POINTS: i32 = 21;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TransformSpec {
  source_definition: String,
  target_definition: String,
}

#[derive(Debug, Clone)]
pub struct ReprojectionPlan {
  transform: Option<TransformSpec>,
  target_spatial_reference: SpatialReferenceInfo,
}

#[derive(Debug)]
pub struct PreparedTransform {
  _source: SpatialRef,
  _target: SpatialRef,
  coord_transform: CoordTransform,
}

impl ReprojectionPlan {
  pub fn from_source_metadata(
    source_metadata: &SourceDatasetMetadata,
    geometry_column: &str,
    target_wkid: u32,
  ) -> Result<Self> {
    let source_projjson = source_projjson_for_geometry(source_metadata, geometry_column)?;
    let source_definition =
      serde_json::to_string(&source_projjson).context("serialize source CRS as PROJJSON")?;
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

  pub fn requires_reprojection(&self) -> bool {
    self.transform.is_some()
  }

  pub fn transform(&self) -> Option<&TransformSpec> {
    self.transform.as_ref()
  }

  pub fn target_spatial_reference(&self) -> &SpatialReferenceInfo {
    &self.target_spatial_reference
  }
}

impl TransformSpec {
  pub fn prepare(&self) -> Result<PreparedTransform> {
    let source = spatial_ref_from_definition(&self.source_definition)?;
    let target = spatial_ref_from_definition(&self.target_definition)?;
    let coord_transform =
      CoordTransform::new(&source, &target).context("build coordinate transform")?;
    Ok(PreparedTransform {
      _source: source,
      _target: target,
      coord_transform,
    })
  }

  pub fn transform_point(&self, x: f64, y: f64) -> Result<(f64, f64)> {
    self.prepare()?.transform_point(x, y)
  }

  pub fn transform_bounds(&self, bounds: Extent2D) -> Result<Extent2D> {
    self.prepare()?.transform_bounds(bounds)
  }

  pub fn transform_geometry_bounds_from_wkb(
    &self,
    bytes: &[u8],
    geometry_type: DisplayGeometryType,
  ) -> Result<Extent2D> {
    self
      .prepare()?
      .transform_geometry_bounds_from_wkb(bytes, geometry_type)
  }

  pub fn reproject_wkb(&self, bytes: &[u8]) -> Result<Vec<u8>> {
    self.prepare()?.reproject_wkb(bytes)
  }
}

impl PreparedTransform {
  pub fn transform_point(&self, x: f64, y: f64) -> Result<(f64, f64)> {
    let mut xs = [x];
    let mut ys = [y];
    self
      .coord_transform
      .transform_coords(&mut xs, &mut ys, &mut [])
      .context("transform point coordinates")?;
    Ok((xs[0], ys[0]))
  }

  pub fn transform_bounds(&self, bounds: Extent2D) -> Result<Extent2D> {
    let [xmin, ymin, xmax, ymax] = self
      .coord_transform
      .transform_bounds(
        &[bounds.xmin, bounds.ymin, bounds.xmax, bounds.ymax],
        TRANSFORM_BOUNDS_DENSIFY_POINTS,
      )
      .context("transform bounding box")?;
    Ok(Extent2D {
      xmin,
      ymin,
      xmax,
      ymax,
    })
  }

  pub fn transform_geometry_bounds_from_wkb(
    &self,
    bytes: &[u8],
    geometry_type: DisplayGeometryType,
  ) -> Result<Extent2D> {
    if matches!(geometry_type, DisplayGeometryType::Point) {
      let (x, y) = crate::pbf::point_xy_from_wkb(bytes)?;
      let (x, y) = self.transform_point(x, y)?;
      return Ok(Extent2D {
        xmin: x,
        ymin: y,
        xmax: x,
        ymax: y,
      });
    }

    let geometry = Geometry::from_wkb(bytes).context("decode geometry for reprojection")?;
    let geometry = geometry
      .transform(&self.coord_transform)
      .context("reproject geometry for bounds")?;
    let envelope = geometry.envelope();
    Ok(Extent2D {
      xmin: envelope.MinX,
      ymin: envelope.MinY,
      xmax: envelope.MaxX,
      ymax: envelope.MaxY,
    })
  }

  pub fn reproject_wkb(&self, bytes: &[u8]) -> Result<Vec<u8>> {
    let geometry = Geometry::from_wkb(bytes).context("decode geometry for reprojection")?;
    let geometry = geometry
      .transform(&self.coord_transform)
      .context("reproject geometry")?;
    geometry.wkb().context("encode reprojected geometry as WKB")
  }
}

fn source_projjson_for_geometry(
  source_metadata: &SourceDatasetMetadata,
  geometry_column: &str,
) -> Result<Value> {
  let geometry = source_metadata
    .geometry
    .as_ref()
    .filter(|geometry| geometry.column == geometry_column)
    .with_context(|| {
      format!("missing geometry metadata for column '{geometry_column}'; CRS metadata is required")
    })?;
  geometry.projjson.clone().with_context(|| {
        format!(
            "missing CRS metadata for geometry column '{geometry_column}'; explicit CRS metadata is required"
        )
    })
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

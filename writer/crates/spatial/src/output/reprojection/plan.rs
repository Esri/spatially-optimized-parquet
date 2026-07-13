//! Plans and executes coordinate-reference transformations through GDAL and PROJ.
//!
//! [`ReprojectionPlan`] validates that the selected geometry has source CRS metadata, constructs
//! the target EPSG definition, and omits transformation when source and target references match.
//! [`TransformSpec`] stores hashable source/target definitions so parameterized DataFusion UDFs
//! can participate in expression equality and physical planning.
//!
//! [`PreparedTransform`] owns the GDAL spatial references and coordinate operation required for
//! repeated batch work. Point and bounds paths avoid unnecessary geometry reconstruction, while
//! general WKB reprojection decodes through GDAL, transforms every coordinate, and re-encodes WKB.
//! Bounds transformations densify edges because nonlinear projections can move extrema away from
//! the original corners.

use anyhow::{Context, Result, bail};
use gdal::spatial_ref::{AxisMappingStrategy, CoordTransform, SpatialRef};
use gdal::vector::Geometry;
use geo_traits::{CoordTrait, GeometryTrait, GeometryType, PointTrait};
use serde_json::Value;

use crate::analysis::{DisplayGeometryType, Extent2D, SpatialReferenceInfo};
use crate::metadata::source::SourceDatasetMetadata;

const TRANSFORM_BOUNDS_DENSIFY_POINTS: i32 = 21;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Stores source and target CRS definitions for deferred transform construction.
pub struct TransformSpec {
  source_definition: String,
  target_definition: String,
}

#[derive(Debug, Clone)]
/// Describes whether a job needs reprojection and the metadata of its target CRS.
pub struct ReprojectionPlan {
  transform: Option<TransformSpec>,
  target_spatial_reference: SpatialReferenceInfo,
}

#[derive(Debug)]
/// Owns a prepared GDAL coordinate transform and the spatial references backing it.
pub struct PreparedTransform {
  _source: SpatialRef,
  _target: SpatialRef,
  coord_transform: CoordTransform,
}

impl ReprojectionPlan {
  /// Resolve source CRS metadata and plan transformation into the target EPSG code.
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
  /// Build reusable GDAL transformation state from the stored CRS definitions.
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

  /// Transform one point, preparing a short-lived transform for this call.
  pub fn transform_point(&self, x: f64, y: f64) -> Result<(f64, f64)> {
    self.prepare()?.transform_point(x, y)
  }

  /// Transform and densify an axis-aligned extent.
  pub fn transform_bounds(&self, bounds: Extent2D) -> Result<Extent2D> {
    self.prepare()?.transform_bounds(bounds)
  }

  /// Decode WKB and calculate its extent in the target CRS.
  pub fn transform_geometry_bounds_from_wkb(
    &self,
    bytes: &[u8],
    geometry_type: DisplayGeometryType,
  ) -> Result<Extent2D> {
    self
      .prepare()?
      .transform_geometry_bounds_from_wkb(bytes, geometry_type)
  }

  /// Decode, transform, and re-encode one WKB geometry.
  pub fn reproject_wkb(&self, bytes: &[u8]) -> Result<Vec<u8>> {
    self.prepare()?.reproject_wkb(bytes)
  }
}

impl PreparedTransform {
  /// Transform one point with the prepared GDAL coordinate operation.
  pub fn transform_point(&self, x: f64, y: f64) -> Result<(f64, f64)> {
    let mut xs = [x];
    let mut ys = [y];
    self
      .coord_transform
      .transform_coords(&mut xs, &mut ys, &mut [])
      .context("transform point coordinates")?;
    Ok((xs[0], ys[0]))
  }

  /// Transform an extent with edge densification to preserve nonlinear extrema.
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

  /// Calculate target-CRS bounds, using a direct point path when possible.
  pub fn transform_geometry_bounds_from_wkb(
    &self,
    bytes: &[u8],
    geometry_type: DisplayGeometryType,
  ) -> Result<Extent2D> {
    if matches!(geometry_type, DisplayGeometryType::Point) {
      let (x, y) = point_xy_from_wkb(bytes)?;
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

  /// Reproject one WKB geometry and return target-CRS WKB.
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

pub(super) fn point_xy_from_wkb(bytes: &[u8]) -> Result<(f64, f64)> {
  let geometry = wkb::reader::read_wkb(bytes)?;
  match geometry.as_type() {
    GeometryType::Point(point) => point
      .coord()
      .map(|coord| coord.x_y())
      .context("point missing coordinate"),
    _ => bail!("expected point geometry"),
  }
}

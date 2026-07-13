//! Executes prepared point, bounds, geometry, and WKB coordinate transformations.

use anyhow::{Context, Result, bail};
use gdal::spatial_ref::{CoordTransform, SpatialRef};
use gdal::vector::Geometry;
use geo_traits::{CoordTrait, GeometryTrait, GeometryType, PointTrait};

use crate::analysis::{DisplayGeometryType, Extent2D};

use super::TransformSpec;

const TRANSFORM_BOUNDS_DENSIFY_POINTS: i32 = 21;

#[derive(Debug)]
/// Owns a prepared GDAL coordinate transform and the spatial references backing it.
pub struct PreparedTransform {
  _source: SpatialRef,
  _target: SpatialRef,
  coord_transform: CoordTransform,
}

impl TransformSpec {
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
  /// Build a coordinate operation backed by source and target spatial references.
  pub(super) fn new(source: SpatialRef, target: SpatialRef) -> Result<Self> {
    let coord_transform =
      CoordTransform::new(&source, &target).context("build coordinate transform")?;
    Ok(Self {
      _source: source,
      _target: target,
      coord_transform,
    })
  }

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

  /// Decode one point from WKB and transform its coordinates.
  pub(crate) fn transform_point_from_wkb(&self, bytes: &[u8]) -> Result<(f64, f64)> {
    let (x, y) = point_xy_from_wkb(bytes)?;
    self.transform_point(x, y)
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
      let (x, y) = self.transform_point_from_wkb(bytes)?;
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

fn point_xy_from_wkb(bytes: &[u8]) -> Result<(f64, f64)> {
  let geometry = wkb::reader::read_wkb(bytes)?;
  match geometry.as_type() {
    GeometryType::Point(point) => point
      .coord()
      .map(|coord| coord.x_y())
      .context("point missing coordinate"),
    _ => bail!("expected point geometry"),
  }
}

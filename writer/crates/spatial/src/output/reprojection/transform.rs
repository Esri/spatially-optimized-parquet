//! Executes prepared point, bounds, geometry, and WKB coordinate transformations.

use anyhow::{Context, Result, bail};
use gdal::spatial_ref::{CoordTransform, SpatialRef};
use gdal::vector::Geometry;
use geo_traits::{CoordTrait, GeometryTrait, GeometryType, PointTrait};

use crate::geometry::{Extent2D, GeometryCategory};

#[derive(Debug)]
/// Owns a prepared GDAL coordinate transform and the spatial references backing it.
pub struct PreparedTransform {
  _source: SpatialRef,
  _target: SpatialRef,
  coord_transform: CoordTransform,
}

impl PreparedTransform {
  /// Construct a coordinate operation backed by source and target spatial references.
  pub(super) fn new(source: SpatialRef, target: SpatialRef) -> Result<Self> {
    let coord_transform =
      CoordTransform::new(&source, &target).context("create coordinate transform")?;
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

  /// Calculate target-CRS bounds, using a direct point path when possible.
  pub fn transform_geometry_bounds_from_wkb(
    &self,
    bytes: &[u8],
    geometry_category: GeometryCategory,
  ) -> Result<Extent2D> {
    if matches!(geometry_category, GeometryCategory::Point) {
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

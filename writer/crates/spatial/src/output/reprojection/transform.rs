//! Executes prepared WKB coordinate transformations.

use anyhow::{Context, Result};
use gdal::spatial_ref::{CoordTransform, SpatialRef};
use gdal::vector::Geometry;

#[derive(Debug)]
/// Owns a prepared GDAL coordinate transform and the spatial references backing it.
pub(super) struct PreparedTransform {
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

  /// Reproject one WKB geometry and return target-CRS WKB.
  pub(super) fn reproject_wkb(&self, bytes: &[u8]) -> Result<Vec<u8>> {
    let geometry = Geometry::from_wkb(bytes).context("decode geometry for reprojection")?;
    let geometry = geometry
      .transform(&self.coord_transform)
      .context("reproject geometry")?;
    geometry.wkb().context("encode reprojected geometry as WKB")
  }
}

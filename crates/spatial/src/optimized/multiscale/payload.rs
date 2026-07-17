//! Creates reusable flat coordinate payloads through shared geometry traversal.

use anyhow::Result;
#[cfg(test)]
use geo_traits::GeometryTrait;
#[cfg(test)]
use geo_types::Geometry;

#[cfg(test)]
use super::traversal::visit_geometry_for_display;
use super::traversal::{
  ExtentAccumulator, GeometryPartRole, GeometryPartSink, visit_wkb_geometry_for_display,
};
use crate::geometry::{Extent2D, WkbCoordinate};
use crate::optimized::OptimizedGeometryType;

/// Stores flattened coordinate and part-length sequences without computed bounds.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct FlatGeometryPayload {
  /// Stores complete coordinates for every traversed part.
  pub(super) coordinates: Vec<WkbCoordinate>,
  /// Stores the coordinate-pair count of each geometry part.
  pub(super) lengths: Vec<u32>,
  pub(super) has_z: bool,
  pub(super) has_m: bool,
}

/// Stores flattened geometry sequences together with their source extent.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq)]
pub(super) struct GeometryPayload {
  /// Stores complete coordinates for every traversed part.
  pub(super) coordinates: Vec<WkbCoordinate>,
  /// Stores the coordinate-pair count of each geometry part.
  pub(super) lengths: Vec<u32>,
  /// Stores the extent observed while flattening coordinates.
  pub(super) bounds: Extent2D,
  pub(super) has_z: bool,
  pub(super) has_m: bool,
}

/// Decode WKB into flattened optimized geometry without calculating bounds.
pub(super) fn flat_geometry_payload_from_wkb(
  bytes: &[u8],
  geometry_type: OptimizedGeometryType,
) -> Result<FlatGeometryPayload> {
  Ok(geometry_payload_parts_from_wkb(bytes, geometry_type, false)?.0)
}

/// Flatten an owned `geo_types` geometry and calculate its bounds.
#[cfg(test)]
pub(super) fn geometry_payload_from_geometry(
  geometry: &Geometry<f64>,
  geometry_type: OptimizedGeometryType,
) -> Result<GeometryPayload> {
  let (payload, bounds) =
    geometry_payload_parts_from_geometry_trait(geometry, geometry_type, true)?;
  Ok(GeometryPayload {
    coordinates: payload.coordinates,
    lengths: payload.lengths,
    bounds: bounds.unwrap_or_default(),
    has_z: payload.has_z,
    has_m: payload.has_m,
  })
}

fn geometry_payload_parts_from_wkb(
  bytes: &[u8],
  geometry_type: OptimizedGeometryType,
  track_bounds: bool,
) -> Result<(FlatGeometryPayload, Option<Extent2D>)> {
  let mut builder = PayloadBuilder::new(track_bounds);
  let dimensions = visit_wkb_geometry_for_display(bytes, geometry_type, &mut builder)?;
  Ok(builder.finish(
    matches!(
      dimensions,
      geo_traits::Dimensions::Xyz | geo_traits::Dimensions::Xyzm
    ),
    matches!(
      dimensions,
      geo_traits::Dimensions::Xym | geo_traits::Dimensions::Xyzm
    ),
  ))
}

#[cfg(test)]
fn geometry_payload_parts_from_geometry_trait<G: GeometryTrait<T = f64>>(
  geometry: &G,
  geometry_type: OptimizedGeometryType,
  track_bounds: bool,
) -> Result<(FlatGeometryPayload, Option<Extent2D>)> {
  let mut builder = PayloadBuilder::new(track_bounds);
  visit_geometry_for_display(geometry, geometry_type, &mut builder)?;
  Ok(builder.finish(false, false))
}

struct PayloadBuilder {
  coordinates: Vec<WkbCoordinate>,
  lengths: Vec<u32>,
  current_len: u32,
  bounds: Option<ExtentAccumulator>,
}

impl PayloadBuilder {
  fn new(track_bounds: bool) -> Self {
    Self {
      coordinates: Vec::new(),
      lengths: Vec::new(),
      current_len: 0,
      bounds: track_bounds.then(ExtentAccumulator::default),
    }
  }

  fn finish(self, has_z: bool, has_m: bool) -> (FlatGeometryPayload, Option<Extent2D>) {
    (
      FlatGeometryPayload {
        coordinates: self.coordinates,
        lengths: self.lengths,
        has_z,
        has_m,
      },
      self.bounds.and_then(ExtentAccumulator::finish),
    )
  }
}

impl GeometryPartSink for PayloadBuilder {
  fn start_part(&mut self, _: GeometryPartRole) {
    self.current_len = 0;
  }

  fn push_coord(&mut self, coordinate: WkbCoordinate) {
    self.coordinates.push(coordinate);
    self.current_len += 1;
    if let Some(bounds) = self.bounds.as_mut() {
      bounds.push(coordinate.x, coordinate.y);
    }
  }

  fn finish_part(&mut self) {
    self.lengths.push(self.current_len);
    self.current_len = 0;
  }
}

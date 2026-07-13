//! Builds reusable flat coordinate payloads through shared geometry traversal.

use anyhow::Result;
use geo_traits::GeometryTrait;
use geo_types::Geometry;

use super::traversal::{ExtentAccumulator, GeometryPartSink, visit_geometry_for_display};
use crate::geometry::Extent2D;
use crate::optimized::OptimizedGeometryType;

/// Stores flattened coordinate and part-length sequences without computed bounds.
#[derive(Debug, Clone, PartialEq)]
pub struct FlatGeometryPayload {
  /// Stores interleaved x/y coordinates for every traversed part.
  pub coords: Vec<f64>,
  /// Stores the coordinate-pair count of each geometry part.
  pub lengths: Vec<u32>,
}

/// Stores flattened geometry sequences together with their source extent.
#[derive(Debug, Clone, PartialEq)]
pub struct GeometryPayload {
  /// Stores interleaved x/y coordinates for every traversed part.
  pub coords: Vec<f64>,
  /// Stores the coordinate-pair count of each geometry part.
  pub lengths: Vec<u32>,
  /// Stores the extent observed while flattening coordinates.
  pub bounds: Extent2D,
}

/// Decode WKB into flattened optimized geometry and bounds.
pub fn geometry_payload_from_wkb(
  bytes: &[u8],
  geometry_type: OptimizedGeometryType,
) -> Result<GeometryPayload> {
  let (payload, bounds) = geometry_payload_parts_from_wkb(bytes, geometry_type, true)?;
  Ok(GeometryPayload {
    coords: payload.coords,
    lengths: payload.lengths,
    bounds: bounds.unwrap_or_default(),
  })
}

/// Decode WKB into flattened optimized geometry without calculating bounds.
pub fn flat_geometry_payload_from_wkb(
  bytes: &[u8],
  geometry_type: OptimizedGeometryType,
) -> Result<FlatGeometryPayload> {
  Ok(geometry_payload_parts_from_wkb(bytes, geometry_type, false)?.0)
}

/// Flatten an owned `geo_types` geometry and calculate its bounds.
pub fn geometry_payload_from_geometry(
  geometry: &Geometry<f64>,
  geometry_type: OptimizedGeometryType,
) -> Result<GeometryPayload> {
  let (payload, bounds) =
    geometry_payload_parts_from_geometry_trait(geometry, geometry_type, true)?;
  Ok(GeometryPayload {
    coords: payload.coords,
    lengths: payload.lengths,
    bounds: bounds.unwrap_or_default(),
  })
}

fn geometry_payload_parts_from_wkb(
  bytes: &[u8],
  geometry_type: OptimizedGeometryType,
  track_bounds: bool,
) -> Result<(FlatGeometryPayload, Option<Extent2D>)> {
  let geometry = wkb::reader::read_wkb(bytes)?;
  geometry_payload_parts_from_geometry_trait(&geometry, geometry_type, track_bounds)
}

fn geometry_payload_parts_from_geometry_trait<G: GeometryTrait<T = f64>>(
  geometry: &G,
  geometry_type: OptimizedGeometryType,
  track_bounds: bool,
) -> Result<(FlatGeometryPayload, Option<Extent2D>)> {
  let mut builder = PayloadBuilder::new(track_bounds);
  visit_geometry_for_display(geometry, geometry_type, &mut builder)?;
  Ok(builder.finish())
}

struct PayloadBuilder {
  coords: Vec<f64>,
  lengths: Vec<u32>,
  current_len: u32,
  bounds: Option<ExtentAccumulator>,
}

impl PayloadBuilder {
  fn new(track_bounds: bool) -> Self {
    Self {
      coords: Vec::new(),
      lengths: Vec::new(),
      current_len: 0,
      bounds: track_bounds.then(ExtentAccumulator::default),
    }
  }

  fn finish(self) -> (FlatGeometryPayload, Option<Extent2D>) {
    (
      FlatGeometryPayload {
        coords: self.coords,
        lengths: self.lengths,
      },
      self.bounds.and_then(ExtentAccumulator::finish),
    )
  }
}

impl GeometryPartSink for PayloadBuilder {
  fn start_part(&mut self) {
    self.current_len = 0;
  }

  fn push_coord(&mut self, x: f64, y: f64) {
    self.coords.push(x);
    self.coords.push(y);
    self.current_len += 1;
    if let Some(bounds) = self.bounds.as_mut() {
      bounds.push(x, y);
    }
  }

  fn finish_part(&mut self) {
    self.lengths.push(self.current_len);
    self.current_len = 0;
  }
}

#[cfg(test)]
mod tests {
  use geo_types::{Geometry, polygon};

  use super::*;

  #[test]
  fn wkb_payload_matches_geometry_payload() {
    let geometry = Geometry::Polygon(polygon![
        (x: 0.0, y: 0.0),
        (x: 2.0, y: 0.0),
        (x: 2.0, y: 1.0),
        (x: 0.0, y: 0.0),
    ]);
    let mut buffer = Vec::new();
    wkb::writer::write_geometry(
      &mut buffer,
      &geometry,
      &wkb::writer::WriteOptions::default(),
    )
    .unwrap();

    let from_geometry =
      geometry_payload_from_geometry(&geometry, OptimizedGeometryType::Polygon).unwrap();
    let from_wkb = geometry_payload_from_wkb(&buffer, OptimizedGeometryType::Polygon).unwrap();
    let flat = flat_geometry_payload_from_wkb(&buffer, OptimizedGeometryType::Polygon).unwrap();

    assert_eq!(from_wkb, from_geometry);
    assert_eq!(flat.coords, from_geometry.coords);
    assert_eq!(flat.lengths, from_geometry.lengths);
  }
}

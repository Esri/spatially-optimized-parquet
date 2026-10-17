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

//! Traverses supported geometry structures through a shared part sink.

use crate::geometry::{
  Coord, CoordinateDimensions, Extent2D, GeometryError, GeometryFamily, GeometryType,
  PolygonRingOrder, WkbHeader, visit_wkb_geometry as decode_wkb_geometry,
};

pub(crate) use crate::geometry::{WkbPartRole as GeometryPartRole, WkbSink as GeometryPartSink};

impl Extent2D {
  /// Calculate the axis-aligned extent of one decoded WKB geometry.
  pub(crate) fn from_wkb(bytes: &[u8]) -> Result<Self, GeometryError> {
    let mut collector = BoundsCollector::default();
    decode_wkb_geometry(bytes, PolygonRingOrder::Preserve, &mut collector)?;
    collector.finish().ok_or_else(|| {
      GeometryError::InvalidGeometry("geometry missing bounding rectangle".to_string())
    })
  }
}

impl WkbHeader {
  /// Visit one WKB geometry through canonical part traversal.
  pub(crate) fn visit<S: GeometryPartSink>(
    bytes: &[u8],
    sink: &mut S,
  ) -> Result<Self, GeometryError> {
    decode_wkb_geometry(bytes, PolygonRingOrder::Preserve, sink)
  }
}

impl GeometryFamily {
  /// Visit one display geometry while enforcing this optimized geometry type.
  pub(crate) fn visit_wkb_for_display<S: GeometryPartSink>(
    self,
    bytes: &[u8],
    sink: &mut S,
  ) -> Result<CoordinateDimensions, GeometryError> {
    let header = decode_wkb_geometry(bytes, PolygonRingOrder::Reverse, sink)?;
    let type_matches = match self {
      Self::Point => header.ty == GeometryType::Point,
      Self::MultiPoint => header.ty == GeometryType::MultiPoint,
      Self::Polyline => {
        matches!(
          header.ty,
          GeometryType::LineString | GeometryType::MultiLineString
        )
      }
      Self::Polygon => {
        matches!(
          header.ty,
          GeometryType::Polygon | GeometryType::MultiPolygon
        )
      }
    };
    if !type_matches {
      return Err(GeometryError::InvalidGeometry(format!(
        "WKB geometry {:?} does not match optimized type {self:?}",
        header.ty
      )));
    }
    Ok(header.dimensions)
  }
}

#[derive(Default)]
struct BoundsCollector {
  extent: Option<Extent2D>,
}

impl BoundsCollector {
  /// Extend the collected XY bounds with one decoded coordinate.
  fn push(&mut self, x: f64, y: f64) {
    match self.extent.as_mut() {
      Some(extent) => {
        extent.xmin = extent.xmin.min(x);
        extent.ymin = extent.ymin.min(y);
        extent.xmax = extent.xmax.max(x);
        extent.ymax = extent.ymax.max(y);
      }
      None => {
        self.extent = Some(Extent2D {
          xmin: x,
          ymin: y,
          xmax: x,
          ymax: y,
        });
      }
    }
  }

  /// Return the XY extent when traversal visited at least one coordinate.
  fn finish(self) -> Option<Extent2D> {
    self.extent
  }
}

impl GeometryPartSink for BoundsCollector {
  /// Ignore part boundaries because all coordinates contribute to the same extent.
  fn start_part(&mut self, _: GeometryPartRole) {}

  /// Include the decoded coordinate's XY position in the collected bounds.
  fn push_coord(&mut self, coordinate: Coord) {
    self.push(coordinate.x, coordinate.y);
  }

  /// Ignore part completion because bounds span the complete geometry.
  fn finish_part(&mut self) {}
}

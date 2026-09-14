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
  CoordinateDimensions, Extent2D, GeometryError, GeometryFamily as GeometryType, GeometryKind,
  PolygonRingOrder, WkbCoordinate, WkbHeader, visit_wkb_geometry as decode_wkb_geometry,
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
    Ok(decode_wkb_geometry(
      bytes,
      PolygonRingOrder::Preserve,
      sink,
    )?)
  }
}

impl GeometryType {
  /// Visit one display geometry while enforcing this optimized geometry type.
  pub(crate) fn visit_wkb_for_display<S: GeometryPartSink>(
    self,
    bytes: &[u8],
    sink: &mut S,
  ) -> Result<CoordinateDimensions, GeometryError> {
    let header = decode_wkb_geometry(bytes, PolygonRingOrder::Reverse, sink)?;
    let kind_matches = match self {
      Self::Point => header.kind == GeometryKind::Point,
      Self::MultiPoint => header.kind == GeometryKind::MultiPoint,
      Self::Polyline => {
        matches!(
          header.kind,
          GeometryKind::LineString | GeometryKind::MultiLineString
        )
      }
      Self::Polygon => {
        matches!(
          header.kind,
          GeometryKind::Polygon | GeometryKind::MultiPolygon
        )
      }
    };
    if !kind_matches {
      return Err(GeometryError::InvalidGeometry(format!(
        "WKB geometry {:?} does not match optimized type {self:?}",
        header.kind
      )));
    }
    Ok(header.dimensions)
  }
}

#[derive(Default)]
struct ExtentAccumulator {
  extent: Option<Extent2D>,
}

impl ExtentAccumulator {
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

  fn finish(self) -> Option<Extent2D> {
    self.extent
  }
}

#[derive(Default)]
struct BoundsCollector {
  bounds: ExtentAccumulator,
}

impl BoundsCollector {
  fn finish(self) -> Option<Extent2D> {
    self.bounds.finish()
  }
}

impl GeometryPartSink for BoundsCollector {
  fn start_part(&mut self, _: GeometryPartRole) {}

  fn push_coord(&mut self, coordinate: WkbCoordinate) {
    self.bounds.push(coordinate.x, coordinate.y);
  }

  fn finish_part(&mut self) {}
}

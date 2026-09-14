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

use crate::geometry::{
  CoordinateDimensions, Extent2D, GeometryFamily as GeometryType, GeometryKind,
};
use crate::geometry::{WkbCoordinate, WkbHeader};
use crate::optimized::{GeometryPartRole, GeometryPartSink};
use arrow_array::{Array, BinaryArray, BinaryViewArray, LargeBinaryArray};

use super::ValidationError;
use super::report::{ValidationLocation, ValidationReport, ValidationRule, ValidationSeverity};

#[derive(Debug, Clone, PartialEq)]
struct RingValidationInfo {
  role: GeometryPartRole,
  closed: bool,
  signed_area: f64,
  degenerated: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GeometryValidationInfo {
  kind: GeometryKind,
  dimensions: CoordinateDimensions,
  pub(crate) extent: Option<Extent2D>,
  finite_coordinates: bool,
  rings: Vec<RingValidationInfo>,
}

pub(crate) struct GeometryValidator;

impl GeometryValidator {
  pub(crate) fn inspect(bytes: &[u8]) -> Result<GeometryValidationInfo, ValidationError> {
    let mut sink = InspectionSink::default();
    let header = WkbHeader::visit(bytes, &mut sink)?;
    Ok(GeometryValidationInfo {
      kind: header.kind,
      dimensions: header.dimensions,
      extent: sink.extent,
      finite_coordinates: sink.has_coordinate && sink.finite_coordinates,
      rings: sink.rings,
    })
  }

  pub(crate) fn validate(
    inspection: &GeometryValidationInfo,
    expected_geometry_type: GeometryType,
    expected_has_z: bool,
    expected_has_m: bool,
    location: ValidationLocation,
    report: &mut ValidationReport,
  ) {
    if !Self::matches_geometry_type(inspection, expected_geometry_type) {
      report.push(
        ValidationRule::GeometryType,
        ValidationSeverity::Error,
        location.clone(),
        format!(
          "sampled WKB geometry {:?} does not match geometryType '{}'",
          inspection.kind,
          expected_geometry_type.as_str()
        ),
      );
    }
    let expected_dimensions = match (expected_has_z, expected_has_m) {
      (false, false) => CoordinateDimensions::Xy,
      (true, false) => CoordinateDimensions::Xyz,
      (false, true) => CoordinateDimensions::Xym,
      (true, true) => CoordinateDimensions::Xyzm,
    };
    if inspection.dimensions != expected_dimensions {
      report.push(
        ValidationRule::GeometryDimension,
        ValidationSeverity::Error,
        location.clone(),
        format!(
          "sampled WKB geometry dimensions {:?} do not match metadata dimensions {:?}",
          inspection.dimensions, expected_dimensions
        ),
      );
    }
    if !inspection.finite_coordinates || inspection.extent.is_none() {
      report.push(
        ValidationRule::GeometryCoordinate,
        ValidationSeverity::Error,
        location.clone(),
        "sampled WKB geometry contains no finite coordinates",
      );
    }
    for ring in &inspection.rings {
      if !ring.closed {
        report.push(
          ValidationRule::RingClosure,
          ValidationSeverity::Error,
          location.clone(),
          "sampled WKB polygon ring is not closed",
        );
      }
      if ring.degenerated {
        continue;
      }
      let expected_positive = ring.role == GeometryPartRole::Exterior;
      if (expected_positive && ring.signed_area < 0.0)
        || (!expected_positive && ring.signed_area > 0.0)
      {
        let role = if expected_positive {
          "exterior"
        } else {
          "interior"
        };
        report.push(
          ValidationRule::WkbWinding,
          ValidationSeverity::Warning,
          location.clone(),
          format!("sampled WKB {role} ring has unexpected winding"),
        );
      }
    }
  }

  pub(crate) fn binary_value(
    array: &dyn Array,
    index: usize,
  ) -> Result<Option<Vec<u8>>, ValidationError> {
    if array.is_null(index) {
      return Ok(None);
    }
    if let Some(array) = array.as_any().downcast_ref::<BinaryArray>() {
      return Ok(Some(array.value(index).to_vec()));
    }
    if let Some(array) = array.as_any().downcast_ref::<LargeBinaryArray>() {
      return Ok(Some(array.value(index).to_vec()));
    }
    if let Some(array) = array.as_any().downcast_ref::<BinaryViewArray>() {
      return Ok(Some(array.value(index).to_vec()));
    }
    Err(ValidationError::ArrowColumn(format!(
      "expected Arrow binary array, found {}",
      array.data_type()
    )))
  }

  fn matches_geometry_type(inspection: &GeometryValidationInfo, expected: GeometryType) -> bool {
    match expected {
      GeometryType::Point => inspection.kind == GeometryKind::Point,
      GeometryType::MultiPoint => inspection.kind == GeometryKind::MultiPoint,
      GeometryType::Polyline => matches!(
        inspection.kind,
        GeometryKind::LineString | GeometryKind::MultiLineString
      ),
      GeometryType::Polygon => matches!(
        inspection.kind,
        GeometryKind::Polygon | GeometryKind::MultiPolygon
      ),
    }
  }
}

struct InspectionSink {
  extent: Option<Extent2D>,
  has_coordinate: bool,
  finite_coordinates: bool,
  current_role: Option<GeometryPartRole>,
  current_coordinates: Vec<(f64, f64)>,
  rings: Vec<RingValidationInfo>,
}

impl Default for InspectionSink {
  fn default() -> Self {
    Self {
      extent: None,
      has_coordinate: false,
      finite_coordinates: true,
      current_role: None,
      current_coordinates: Vec::new(),
      rings: Vec::new(),
    }
  }
}

impl InspectionSink {
  fn signed_area(coordinates: &[(f64, f64)]) -> f64 {
    if coordinates.len() < 3 {
      return 0.0;
    }
    let mut twice_area = 0.0;
    for pair in coordinates.windows(2) {
      twice_area += pair[0].0 * pair[1].1 - pair[1].0 * pair[0].1;
    }
    if coordinates.first() != coordinates.last() {
      let first = coordinates[0];
      let last = coordinates[coordinates.len() - 1];
      twice_area += last.0 * first.1 - first.0 * last.1;
    }
    twice_area / 2.0
  }
}

impl GeometryPartSink for InspectionSink {
  fn start_part(&mut self, role: GeometryPartRole) {
    self.current_role = Some(role);
    self.current_coordinates.clear();
  }

  fn push_coord(&mut self, coordinate: WkbCoordinate) {
    let x = coordinate.x;
    let y = coordinate.y;
    self.has_coordinate = true;
    self.finite_coordinates &= x.is_finite() && y.is_finite();
    if x.is_finite() && y.is_finite() {
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
    self.current_coordinates.push((x, y));
  }

  fn finish_part(&mut self) {
    let role = self.current_role.take().unwrap_or(GeometryPartRole::Other);
    if role == GeometryPartRole::Other {
      self.current_coordinates.clear();
      return;
    }
    let closed = self.current_coordinates.len() >= 2
      && self.current_coordinates.first() == self.current_coordinates.last();
    let signed_area = Self::signed_area(&self.current_coordinates);
    let degenerated = self.current_coordinates.len() < 4 || signed_area.abs() <= f64::EPSILON;
    self.rings.push(RingValidationInfo {
      role,
      closed,
      signed_area,
      degenerated,
    });
    self.current_coordinates.clear();
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn geometry_inspection_tracks_ring_winding() {
    let exterior = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 0.0)];
    let bytes = crate::geometry::write_test_polygon(&[&exterior]);

    let inspection = GeometryValidator::inspect(&bytes).unwrap();

    assert_eq!(inspection.kind, GeometryKind::Polygon);
    assert!(inspection.rings[0].closed);
    assert!(inspection.rings[0].signed_area > 0.0);
  }
}

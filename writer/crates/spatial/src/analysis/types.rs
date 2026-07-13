use serde::Serialize;
use serde_json::Value;

use crate::geometry::GeometrySpec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Groups geometry types by the output indexing and encoding strategy they require.
pub enum GeometryFamily {
  /// Uses scalar x/y columns and Morton Z-order indexing.
  Point,
  /// Uses bounds, XZ-order indexing, and multiscale geometry payloads.
  NonPoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// Identifies the display geometry categories supported by spatial optimization.
pub enum DisplayGeometryType {
  /// Represents single-point features.
  Point,
  /// Represents multipoint features.
  MultiPoint,
  /// Represents line string and multi-line string features.
  Polyline,
  /// Represents polygon and multipolygon features.
  Polygon,
}

impl DisplayGeometryType {
  /// Return the canonical metadata label for this display type.
  pub fn as_str(self) -> &'static str {
    match self {
      Self::Point => "point",
      Self::MultiPoint => "multipoint",
      Self::Polyline => "polyline",
      Self::Polygon => "polygon",
    }
  }

  /// Return the output strategy family for this display type.
  pub fn family(self) -> GeometryFamily {
    match self {
      Self::Point => GeometryFamily::Point,
      Self::MultiPoint | Self::Polyline | Self::Polygon => GeometryFamily::NonPoint,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Default)]
/// Represents an axis-aligned two-dimensional extent.
pub struct Extent2D {
  /// Stores the minimum x coordinate.
  pub xmin: f64,
  /// Stores the minimum y coordinate.
  pub ymin: f64,
  /// Stores the maximum x coordinate.
  pub xmax: f64,
  /// Stores the maximum y coordinate.
  pub ymax: f64,
}

impl Extent2D {
  pub(super) fn expand_to_include(&mut self, other: &Self) {
    self.xmin = self.xmin.min(other.xmin);
    self.ymin = self.ymin.min(other.ymin);
    self.xmax = self.xmax.max(other.xmax);
    self.ymax = self.ymax.max(other.ymax);
  }
}

#[derive(Debug, Clone, PartialEq, Default)]
/// Stores equivalent identifiers and definitions for one coordinate reference system.
pub struct SpatialReferenceInfo {
  /// Stores an EPSG well-known identifier when one can be inferred.
  pub wkid: Option<u32>,
  /// Stores a WKT definition when available.
  pub wkt: Option<String>,
  /// Stores the authoritative PROJJSON definition.
  pub projjson: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
/// Captures the geometry facts required to build display columns and output metadata.
pub struct DisplayJobAnalysis {
  /// Stores the selected source geometry column.
  pub geometry_spec: GeometrySpec,
  /// Stores the display category used by metadata and encoders.
  pub geometry_type: DisplayGeometryType,
  /// Stores the output strategy family.
  pub geometry_family: GeometryFamily,
  /// Stores the extent in the output coordinate reference system.
  pub full_extent: Extent2D,
  /// Stores the output coordinate reference system.
  pub spatial_reference: SpatialReferenceInfo,
  /// Indicates whether source metadata declares Z ordinates.
  pub has_z: bool,
  /// Indicates whether source metadata declares M ordinates.
  pub has_m: bool,
}

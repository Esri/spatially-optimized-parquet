#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Identifies the concrete geometry shape represented by source metadata or WKB.
pub(crate) enum GeometryKind {
  /// Represents one point.
  Point,
  /// Represents one line string.
  LineString,
  /// Represents multiple points.
  MultiPoint,
  /// Represents multiple line strings.
  MultiLineString,
  /// Represents one polygon.
  Polygon,
  /// Represents multiple polygons.
  MultiPolygon,
  /// Represents a heterogeneous geometry collection.
  GeometryCollection,
  /// Represents a geometry whose concrete type cannot be established.
  Unknown,
}

#[derive(Debug, Clone, PartialEq)]
/// Describes the selected geometry column and its source encoding.
pub(crate) struct GeometryColumn {
  /// Identifies the Arrow column containing geometry values.
  pub(crate) column: String,
  /// Defines the physical encoding used by that column.
  pub(crate) encoding: GeometryEncoding,
  /// Provides the source geometry kind when metadata can determine it.
  pub(crate) geometry_kind: Option<GeometryKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Identifies supported physical geometry encodings.
pub(crate) enum GeometryEncoding {
  /// Represents Open Geospatial Consortium Well-Known Binary.
  Wkb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Identifies the coordinate dimensions encoded by a geometry.
pub(crate) enum CoordinateDimensions {
  Xy,
  Xyz,
  Xym,
  Xyzm,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Identifies the concrete geometry shape represented by source metadata or WKB.
pub(crate) enum GeometryType {
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

/// Preserves the previous name while call sites migrate to `GeometryType`.
pub(crate) type GeometryKind = GeometryType;

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

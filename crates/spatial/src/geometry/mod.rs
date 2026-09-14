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

//! Defines the format-neutral geometry model and its physical codecs.
//!
//! [`geometry::Geometry`] carries floating-point coordinates and part boundaries. WKB reads and
//! writes that value, [`quantization`] converts it into integer coordinates, and [`native`] plus
//! [`pbf`] encode those integers as nested Arrow values or Esri PBF bytes. This module owns
//! representation mechanics. Multiscale policy and output selection remain outside it.

mod arrow;
mod error;
mod extent;
mod geometry;
mod native;
mod pbf;
mod quantization;
mod types;
mod wkb;

pub(crate) use arrow::{GeometryArray, geometry_signature, to_datafusion_error};
pub use error::GeometryError;
pub(crate) use extent::Extent2D;
pub(crate) use geometry::{Coord, Geometry, GeometryFamily};
pub(crate) use native::NativeGeometryArrayBuilder;
pub(crate) use pbf::{PbfArrayBuilder, PbfGeometry};
pub(crate) use quantization::{
  ComponentValidity, QuantizationOptions, QuantizationTransform, QuantizedGeometry,
  encode_deltas_xy,
};
pub(crate) use types::{
  CoordinateDimensions, GeometryColumn, GeometryEncoding, GeometryKind, GeometryType,
};
pub(crate) use wkb::{
  PolygonRingOrder, WkbArrayBuilder, WkbCoordinate, WkbHeader, WkbPartRole, WkbSink,
  strip_wkb_dimensions, visit_wkb_geometry,
};
#[cfg(test)]
pub(crate) use wkb::{write_test_point, write_test_polygon};

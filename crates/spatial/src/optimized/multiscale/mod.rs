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

//! Owns geodisplay columns, level planning, geometry traversal, and multiscale encoding.

mod columns;
mod datafusion;
mod encoding;
mod level_array_builder;
mod levels;
mod traversal;

pub(crate) use columns::{
  GEODISPLAY_COLUMN, GEOKEY_COLUMN, GEOLOD_COLUMN, POINT_M_COLUMN, POINT_X_COLUMN, POINT_Y_COLUMN,
  POINT_Z_COLUMN, SOP_GEOMETRY_COLUMN,
};
pub(crate) use datafusion::{GeolodUdf, SopGeometryUdf};
pub use encoding::MultiscaleEncoding;
pub(crate) use levels::MultiscaleLevel;
pub(crate) use traversal::{GeometryPartRole, GeometryPartSink};

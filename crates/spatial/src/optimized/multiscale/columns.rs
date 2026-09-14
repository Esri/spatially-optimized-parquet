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

//! Names generated and intermediate columns used by optimized multiscale output.

/// Defines the persisted spatial ordering key column.
pub(crate) const GEOKEY_COLUMN: &str = "geokey";
/// Defines the generated point x-coordinate column.
pub(crate) const POINT_X_COLUMN: &str = "x";
/// Defines the generated point y-coordinate column.
pub(crate) const POINT_Y_COLUMN: &str = "y";
/// Defines the generated point z-coordinate column.
pub(crate) const POINT_Z_COLUMN: &str = "z";
/// Defines the generated point m-coordinate column.
pub(crate) const POINT_M_COLUMN: &str = "m";
/// Defines the persisted point coordinate struct column.
pub(crate) const SOP_GEOMETRY_COLUMN: &str = "sop_geometry";
/// Defines the persisted multiscale geometry struct column.
pub(crate) const GEOLOD_COLUMN: &str = "geolod";
/// Defines the legacy generated geodisplay struct column.
pub(crate) const GEODISPLAY_COLUMN: &str = "geodisplay";

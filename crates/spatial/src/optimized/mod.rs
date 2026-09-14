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

//! Implements optimized spatial analysis, projection, and physical output mechanics.

mod clustering;
pub(crate) mod geodisplay_metadata;
mod geometry_info;
mod layout;
mod metadata;
mod multiscale;
mod select;

pub(crate) use clustering::{BoundsUdf, ClusterKey, ClusterRangeBoundaries, PointGeometryUdf};
pub(crate) use geodisplay_metadata::{
  ClusteringIndexXZ, ClusteringIndexZ, ColumnPath, GEODISPLAY_VERSION, GeodisplayEncoding,
  GeodisplayMetadata, MultiscaleLevel,
};
pub(crate) use layout::OptimizedLayout;
#[cfg(test)]
pub(crate) use multiscale::GEOKEY_COLUMN;
pub use multiscale::MultiscaleEncoding;
pub(crate) use multiscale::{GeometryPartRole, GeometryPartSink};

pub(crate) use geometry_info::{ClusteringFamily, GeometryInfo};

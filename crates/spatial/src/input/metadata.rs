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

//! Stores format-neutral metadata normalized by input providers.

use parquet::file::metadata::KeyValue;
use serde_json::Value;

use crate::geometry::{Extent2D, GeometryEncoding, GeometryKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceCoveringMetadata {
  pub(crate) column: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SourceGeometryMetadata {
  pub(crate) column: String,
  pub(crate) encoding: GeometryEncoding,
  pub(crate) geometry_types: Vec<GeometryKind>,
  pub(crate) bbox: Option<Extent2D>,
  pub(crate) covering: Option<SourceCoveringMetadata>,
  pub(crate) projjson: Option<Value>,
  pub(crate) has_z: bool,
  pub(crate) has_m: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct SourceDatasetMetadata {
  pub(crate) geometry: Option<SourceGeometryMetadata>,
  pub(crate) passthrough_kv: Vec<KeyValue>,
}

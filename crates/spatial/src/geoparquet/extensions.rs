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

//! Defines draft GeoParquet spatial ordering and level-of-detail metadata.

use serde::{Deserialize, Serialize};

/// Represents one draft GeoParquet spatial ordering contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub(crate) enum OrderingMetadata {
  #[serde(rename = "z")]
  Z(ZOrderingMetadata),
  #[serde(rename = "xz")]
  Xz(XzOrderingMetadata),
}

/// Describes a point Z-order key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ZOrderingMetadata {
  pub(crate) geometry_column: String,
  pub(crate) extent: [f64; 4],
  pub(crate) bit_width: u8,
}

/// Describes an extent-based XZ-order key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct XzOrderingMetadata {
  pub(crate) geometry_column: String,
  pub(crate) extent: [f64; 4],
  pub(crate) max_level: u8,
}

/// Describes level-of-detail payload encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum LodEncoding {
  #[serde(rename = "pbf")]
  Pbf,
}

/// Describes derived line or polygon geometry payloads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct LodMetadata {
  pub(crate) geometry_column: String,
  pub(crate) encoding: LodEncoding,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) orientation: Option<String>,
  pub(crate) levels: Vec<LodLevel>,
}

/// Describes one derived geometry payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct LodLevel {
  pub(crate) column: [String; 2],
  pub(crate) resolution: f64,
  pub(crate) transform: LodTransform,
}

/// Describes the quantization transform for one level-of-detail payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct LodTransform {
  pub(crate) scale: [f64; 4],
  pub(crate) translate: [f64; 4],
}

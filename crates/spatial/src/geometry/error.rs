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

/// Represents failures while parsing, transforming, or encoding geometry values.
#[derive(Debug, thiserror::Error)]
pub enum GeometryError {
  /// Reports malformed or unsupported WKB input.
  #[error("WKB error: {0}")]
  Wkb(String),
  /// Reports invalid geometry topology or dimensions.
  #[error("invalid geometry: {0}")]
  InvalidGeometry(String),
  /// Reports quantization failures.
  #[error("geometry quantization failed: {0}")]
  Quantization(String),
  /// Reports PBF encoding failures.
  #[error("PBF encoding failed: {0}")]
  Pbf(String),
  /// Reports Arrow array construction failures.
  #[error("Arrow geometry operation failed: {0}")]
  Arrow(String),
}

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

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
/// Represents an axis-aligned two-dimensional extent.
pub(crate) struct Extent2D {
  /// Defines the minimum x coordinate.
  pub(crate) xmin: f64,
  /// Defines the minimum y coordinate.
  pub(crate) ymin: f64,
  /// Defines the maximum x coordinate.
  pub(crate) xmax: f64,
  /// Defines the maximum y coordinate.
  pub(crate) ymax: f64,
}

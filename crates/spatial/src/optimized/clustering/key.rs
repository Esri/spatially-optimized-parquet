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

//! Defines the sortable value produced by Z and XZ clustering algorithms.

/// Represents a sortable Z or XZ clustering key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ClusterKey(u64);

impl ClusterKey {
  /// Construct a clustering key from its encoded value.
  pub(crate) const fn new(value: u64) -> Self {
    Self(value)
  }

  /// Return the encoded clustering value.
  pub(crate) const fn value(self) -> u64 {
    self.0
  }
}

impl From<u64> for ClusterKey {
  fn from(value: u64) -> Self {
    Self::new(value)
  }
}

impl From<ClusterKey> for u64 {
  fn from(key: ClusterKey) -> Self {
    key.value()
  }
}

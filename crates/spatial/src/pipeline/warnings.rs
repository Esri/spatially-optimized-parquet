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

//! Collects deduplicated warnings produced during parallel pipeline execution.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

/// Collects deduplicated warning messages for one pipeline execution.
#[derive(Clone, Debug, Default)]
pub(crate) struct PipelineWarnings {
  messages: Arc<Mutex<BTreeSet<String>>>,
}

impl PipelineWarnings {
  pub(crate) fn record(&self, message: String) {
    self
      .messages
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .insert(message);
  }

  pub(crate) fn messages(&self) -> Vec<String> {
    self
      .messages
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner())
      .iter()
      .cloned()
      .collect()
  }
}

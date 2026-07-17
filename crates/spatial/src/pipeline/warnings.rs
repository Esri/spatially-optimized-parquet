//! Collects deduplicated warnings produced during parallel pipeline execution.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

/// Stores deduplicated warning messages for one pipeline execution.
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

//! Defines the durable result returned by the public spatial pipeline.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

/// Stores deduplicated warnings produced during parallel pipeline execution.
#[derive(Clone, Debug, Default)]
pub(crate) struct PipelineWarningStore {
  messages: Arc<Mutex<BTreeSet<String>>>,
}

impl PipelineWarningStore {
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

/// Represents the durable result produced by one spatial pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpatialPipelineResult {
  rows_expected: u64,
  rows_written: u64,
  warnings: Vec<String>,
}

impl SpatialPipelineResult {
  pub(super) fn new(
    rows_expected: u64,
    rows_written: u64,
    warning_store: &PipelineWarningStore,
  ) -> Self {
    Self {
      rows_expected,
      rows_written,
      warnings: warning_store.messages(),
    }
  }

  /// Return the expected number of selected output rows.
  pub const fn rows_expected(&self) -> u64 {
    self.rows_expected
  }

  /// Return the number of rows accepted by the output writer.
  pub const fn rows_written(&self) -> u64 {
    self.rows_written
  }

  /// Return deduplicated warnings collected during pipeline execution.
  pub fn warnings(&self) -> &[String] {
    &self.warnings
  }
}

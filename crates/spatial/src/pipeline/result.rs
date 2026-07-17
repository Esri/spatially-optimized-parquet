//! Defines the durable result returned by the public spatial pipeline.

use super::PipelineWarnings;

/// Represents the durable result produced by one spatial pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpatialPipelineResult {
  rows_expected: u64,
  rows_written: u64,
  warnings: Vec<String>,
}

impl SpatialPipelineResult {
  pub(super) fn new(rows_expected: u64, rows_written: u64, warnings: &PipelineWarnings) -> Self {
    Self {
      rows_expected,
      rows_written,
      warnings: warnings.messages(),
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

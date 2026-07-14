//! Defines the durable result returned by the public spatial pipeline.

use crate::validate::ValidationReport;

/// Represents the durable result produced by one spatial pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpatialPipelineResult {
  rows_written: u64,
  validation_report: Option<ValidationReport>,
}

impl SpatialPipelineResult {
  pub(super) const fn new(rows_written: u64, validation_report: Option<ValidationReport>) -> Self {
    Self {
      rows_written,
      validation_report,
    }
  }

  /// Return the number of rows accepted by the output writer.
  pub const fn rows_written(&self) -> u64 {
    self.rows_written
  }

  /// Return the automatic SOP validation report for optimized output.
  pub const fn validation_report(&self) -> Option<&ValidationReport> {
    self.validation_report.as_ref()
  }
}

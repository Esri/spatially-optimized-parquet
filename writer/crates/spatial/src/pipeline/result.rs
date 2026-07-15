//! Defines the durable result returned by the public spatial pipeline.

/// Represents the durable result produced by one spatial pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpatialPipelineResult {
  rows_expected: u64,
  rows_written: u64,
}

impl SpatialPipelineResult {
  pub(super) const fn new(rows_expected: u64, rows_written: u64) -> Self {
    Self {
      rows_expected,
      rows_written,
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
}

//! Defines the durable result returned by the public spatial pipeline.

/// Represents the durable result produced by one spatial pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpatialPipelineResult {
  rows_written: u64,
}

impl SpatialPipelineResult {
  pub(super) const fn new(rows_written: u64) -> Self {
    Self { rows_written }
  }

  /// Return the number of rows accepted by the output writer.
  pub const fn rows_written(self) -> u64 {
    self.rows_written
  }
}

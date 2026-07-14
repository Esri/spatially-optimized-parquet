//! Defines optional row-count reporting for durable output writes.

use std::sync::Arc;

/// Represents cumulative write progress for one spatial pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriteProgress {
  rows_written: u64,
  total_rows: u64,
}

impl WriteProgress {
  pub(crate) const fn new(rows_written: u64, total_rows: u64) -> Self {
    Self {
      rows_written,
      total_rows,
    }
  }

  /// Return the cumulative number of rows accepted by the writer.
  pub const fn rows_written(self) -> u64 {
    self.rows_written
  }

  /// Return the expected number of rows for the complete write.
  pub const fn total_rows(self) -> u64 {
    self.total_rows
  }
}

/// Receives cumulative row counts without coupling the library to terminal output.
pub trait WriteReporter: Send + Sync {
  /// Report cumulative progress for one durable output write.
  fn report(&self, progress: WriteProgress);
}

impl<Callback> WriteReporter for Callback
where
  Callback: Fn(WriteProgress) + Send + Sync,
{
  fn report(&self, progress: WriteProgress) {
    self(progress);
  }
}

pub(crate) type SharedWriteReporter = Arc<dyn WriteReporter>;

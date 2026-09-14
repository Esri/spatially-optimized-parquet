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

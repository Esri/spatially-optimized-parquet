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

//! Reports monotonically increasing row counts from concurrent Parquet sink writes.

use std::sync::{
  Mutex,
  atomic::{AtomicU64, Ordering},
};

use crate::pipeline::{SharedWriteReporter, WriteProgress};

pub(super) struct WriteReporter {
  total_rows: u64,
  rows_written: AtomicU64,
  callback: Option<SharedWriteReporter>,
  delivered_rows: Mutex<u64>,
}

impl WriteReporter {
  pub(super) fn new(total_rows: u64, callback: Option<SharedWriteReporter>) -> Self {
    Self {
      total_rows,
      rows_written: AtomicU64::new(0),
      callback,
      delivered_rows: Mutex::new(0),
    }
  }

  pub(super) fn record_batch(&self, row_count: usize) {
    if row_count == 0 {
      return;
    }
    let rows_written = self
      .rows_written
      .fetch_add(row_count as u64, Ordering::Relaxed)
      + row_count as u64;
    self.deliver(rows_written, false);
  }

  pub(super) fn finish(&self, rows_written: u64) {
    self.rows_written.store(rows_written, Ordering::Relaxed);
    self.deliver(rows_written, true);
  }

  fn deliver(&self, rows_written: u64, force: bool) {
    let Some(callback) = &self.callback else {
      return;
    };
    let mut delivered_rows = self
      .delivered_rows
      .lock()
      .unwrap_or_else(|poisoned| poisoned.into_inner());
    if rows_written < *delivered_rows || (!force && rows_written == *delivered_rows) {
      return;
    }
    *delivered_rows = rows_written;
    callback.report(WriteProgress::new(rows_written, self.total_rows));
  }
}

#[cfg(test)]
mod tests {
  use std::sync::{Arc, Mutex};

  use crate::pipeline::{SharedWriteReporter, WriteProgress};

  use super::WriteReporter;

  #[test]
  fn reports_monotonic_counts_across_concurrent_writes() {
    let reported = Arc::new(Mutex::new(Vec::new()));
    let callback_rows = Arc::clone(&reported);
    let callback: SharedWriteReporter = Arc::new(move |progress: WriteProgress| {
      callback_rows.lock().unwrap().push(progress.rows_written());
    });
    let reporter = Arc::new(WriteReporter::new(8, Some(callback)));
    let mut writers = Vec::new();
    for _ in 0..8 {
      let reporter = Arc::clone(&reporter);
      writers.push(std::thread::spawn(move || reporter.record_batch(1)));
    }
    for writer in writers {
      writer.join().unwrap();
    }
    reporter.finish(8);

    let reported = reported.lock().unwrap();
    assert_eq!(reported.last(), Some(&8));
    assert!(reported.windows(2).all(|counts| counts[0] <= counts[1]));
  }

  #[test]
  fn finishes_with_authoritative_count() {
    let reported = Arc::new(Mutex::new(Vec::new()));
    let callback_rows = Arc::clone(&reported);
    let callback: SharedWriteReporter = Arc::new(move |progress: WriteProgress| {
      callback_rows.lock().unwrap().push(progress);
    });
    let reporter = WriteReporter::new(3, Some(callback));
    reporter.record_batch(3);
    reporter.finish(3);

    let reported = reported.lock().unwrap();
    assert_eq!(reported.last(), Some(&WriteProgress::new(3, 3)));
    assert_eq!(reported.len(), 2);
  }
}

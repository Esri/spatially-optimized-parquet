//! Owns tracked Parquet sink construction and failed-write cleanup.

use std::any::Any;
use std::fmt;
use std::sync::{
  Arc, Mutex,
  atomic::{AtomicU64, Ordering},
};

use anyhow::Result;
use arrow_schema::{DataType, SchemaRef};
use async_trait::async_trait;
use datafusion::common::{
  DataFusionError, Result as DataFusionResult, config::TableParquetOptions,
};
use datafusion::datasource::file_format::parquet::ParquetSink;
use datafusion::datasource::listing::ListingTableUrl;
use datafusion::datasource::physical_plan::FileSinkConfig;
use datafusion::datasource::sink::DataSink;
use datafusion::execution::TaskContext;
use datafusion::logical_expr::dml::InsertOp;
use datafusion::physical_plan::{
  DisplayAs, DisplayFormatType, SendableRecordBatchStream, stream::RecordBatchStreamAdapter,
};
use futures_util::StreamExt;

use crate::pipeline::{SharedWriteReporter, WriteProgress};

pub(super) struct WriteTracker {
  total_rows: u64,
  rows_written: AtomicU64,
  reporter: Option<SharedWriteReporter>,
  delivered_rows: Mutex<u64>,
}

impl WriteTracker {
  pub(super) fn new(total_rows: u64, reporter: Option<SharedWriteReporter>) -> Self {
    Self {
      total_rows,
      rows_written: AtomicU64::new(0),
      reporter,
      delivered_rows: Mutex::new(0),
    }
  }

  fn record_batch(&self, row_count: usize) {
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
    let Some(reporter) = &self.reporter else {
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
    reporter.report(WriteProgress::new(rows_written, self.total_rows));
  }
}

/// Wraps DataFusion's Parquet sink with shared row tracking and failed-write cleanup.
pub(super) struct TrackingParquetSink {
  config: FileSinkConfig,
  inner: ParquetSink,
  tracker: Arc<WriteTracker>,
}

impl TrackingParquetSink {
  fn new(
    config: FileSinkConfig,
    parquet_options: TableParquetOptions,
    tracker: Arc<WriteTracker>,
  ) -> Self {
    Self {
      config: config.clone(),
      inner: ParquetSink::new(config, parquet_options),
      tracker,
    }
  }

  pub(super) async fn cleanup_written_files(
    &self,
    context: &Arc<TaskContext>,
  ) -> DataFusionResult<()> {
    let object_store = context
      .runtime_env()
      .object_store(&self.config.object_store_url)?;
    let mut cleanup_error = None;
    for path in self.inner.written().keys() {
      if let Err(error) = object_store.delete(path).await
        && cleanup_error.is_none()
      {
        cleanup_error = Some(DataFusionError::ObjectStore(Box::new(error)));
      }
    }
    cleanup_error.map_or(Ok(()), Err)
  }
}

impl fmt::Debug for TrackingParquetSink {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("TrackingParquetSink")
      .finish_non_exhaustive()
  }
}

impl DisplayAs for TrackingParquetSink {
  fn fmt_as(
    &self,
    format_type: DisplayFormatType,
    formatter: &mut fmt::Formatter<'_>,
  ) -> fmt::Result {
    self.inner.fmt_as(format_type, formatter)
  }
}

#[async_trait]
impl DataSink for TrackingParquetSink {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn schema(&self) -> &SchemaRef {
    self.inner.schema()
  }

  async fn write_all(
    &self,
    data: SendableRecordBatchStream,
    context: &Arc<TaskContext>,
  ) -> DataFusionResult<u64> {
    let schema = Arc::clone(self.inner.schema());
    let tracker = Arc::clone(&self.tracker);
    let tracked_stream = data.map(move |batch| {
      if let Ok(batch) = &batch {
        tracker.record_batch(batch.num_rows());
      }
      batch
    });
    self
      .inner
      .write_all(
        Box::pin(RecordBatchStreamAdapter::new(schema, tracked_stream)),
        context,
      )
      .await
  }
}

pub(super) fn create_sink(
  write_path: String,
  output_schema: SchemaRef,
  partition_by: Vec<String>,
  writer_options: TableParquetOptions,
  tracker: Arc<WriteTracker>,
) -> Result<Arc<TrackingParquetSink>> {
  let parsed_url = ListingTableUrl::parse(&write_path)?;
  let config = FileSinkConfig {
    original_url: write_path,
    object_store_url: parsed_url.object_store(),
    file_group: Default::default(),
    table_paths: vec![parsed_url],
    output_schema,
    table_partition_cols: partition_by
      .into_iter()
      .map(|column| (column, DataType::Null))
      .collect(),
    insert_op: InsertOp::Append,
    keep_partition_by_columns: false,
    file_extension: "parquet".to_string(),
  };
  Ok(Arc::new(TrackingParquetSink::new(
    config,
    writer_options,
    tracker,
  )))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn tracker_reports_monotonic_counts_across_concurrent_writes() {
    let reported = Arc::new(Mutex::new(Vec::new()));
    let callback_rows = Arc::clone(&reported);
    let reporter: SharedWriteReporter = Arc::new(move |progress: WriteProgress| {
      callback_rows.lock().unwrap().push(progress.rows_written());
    });
    let tracker = Arc::new(WriteTracker::new(8, Some(reporter)));
    let mut writers = Vec::new();
    for _ in 0..8 {
      let tracker = Arc::clone(&tracker);
      writers.push(std::thread::spawn(move || tracker.record_batch(1)));
    }
    for writer in writers {
      writer.join().unwrap();
    }
    tracker.finish(8);

    let reported = reported.lock().unwrap();
    assert_eq!(reported.last(), Some(&8));
    assert!(reported.windows(2).all(|counts| counts[0] <= counts[1]));
  }

  #[test]
  fn tracker_finishes_with_authoritative_count() {
    let reported = Arc::new(Mutex::new(Vec::new()));
    let callback_rows = Arc::clone(&reported);
    let reporter: SharedWriteReporter = Arc::new(move |progress: WriteProgress| {
      callback_rows.lock().unwrap().push(progress);
    });
    let tracker = WriteTracker::new(3, Some(reporter));
    tracker.record_batch(3);
    tracker.finish(3);

    let reported = reported.lock().unwrap();
    assert_eq!(reported.last(), Some(&WriteProgress::new(3, 3)));
    assert_eq!(reported.len(), 2);
  }
}

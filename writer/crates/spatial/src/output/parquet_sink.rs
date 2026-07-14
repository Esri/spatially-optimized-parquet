//! Owns observable Parquet writes for every output product.

use std::any::Any;
use std::fmt;
use std::sync::{
  Arc, Mutex,
  atomic::{AtomicU64, Ordering},
};

use anyhow::{Context, Result, anyhow};
use arrow_array::{Array, RecordBatch, UInt64Array};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use async_trait::async_trait;
use datafusion::common::{
  DataFusionError, Result as DataFusionResult, config::TableParquetOptions,
};
use datafusion::dataframe::DataFrame;
use datafusion::datasource::file_format::parquet::ParquetSink;
use datafusion::datasource::listing::ListingTableUrl;
use datafusion::datasource::physical_plan::FileSinkConfig;
use datafusion::datasource::sink::{DataSink, DataSinkExec};
use datafusion::execution::TaskContext;
use datafusion::logical_expr::dml::InsertOp;
use datafusion::physical_expr::{Distribution, EquivalenceProperties};
use datafusion::physical_plan::{
  DisplayAs, DisplayFormatType, ExecutionPlan, ExecutionPlanProperties, Partitioning,
  PlanProperties, SendableRecordBatchStream,
  coalesce_partitions::CoalescePartitionsExec,
  collect, execute_input_stream,
  execution_plan::{EvaluationType, SchedulingType},
  sorts::sort_preserving_merge::SortPreservingMergeExec,
  stream::RecordBatchStreamAdapter,
};
use futures_util::StreamExt;
use tokio::task::JoinSet;

use crate::pipeline::{SharedWriteReporter, WriteProgress};

struct WriteTracker {
  total_rows: u64,
  rows_written: AtomicU64,
  reporter: Option<SharedWriteReporter>,
  delivered_rows: Mutex<u64>,
}

impl WriteTracker {
  fn new(total_rows: u64, reporter: Option<SharedWriteReporter>) -> Self {
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

  fn finish(&self, rows_written: u64) {
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
struct TrackingParquetSink {
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

  async fn cleanup_written_files(&self, context: &Arc<TaskContext>) -> DataFusionResult<()> {
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

/// Executes single-file and concurrent partitioned Parquet writes through one tracking sink.
pub(crate) struct TrackingParquetWriter {
  tracker: Arc<WriteTracker>,
}

impl TrackingParquetWriter {
  /// Construct one writer with a shared cumulative row counter.
  pub(crate) fn new(total_rows: u64, reporter: Option<SharedWriteReporter>) -> Self {
    Self {
      tracker: Arc::new(WriteTracker::new(total_rows, reporter)),
    }
  }

  /// Write one DataFrame into an exact Parquet file path.
  pub(crate) async fn write_single(
    self,
    dataframe: DataFrame,
    write_path: String,
    writer_options: TableParquetOptions,
  ) -> Result<u64> {
    let (state, logical_plan) = dataframe.into_parts();
    let context = Arc::new(TaskContext::from(&state));
    let mut input = state.create_physical_plan(&logical_plan).await?;
    if input.output_partitioning().partition_count() != 1 {
      input = match input.properties().output_ordering().cloned() {
        Some(ordering) => Arc::new(SortPreservingMergeExec::new(ordering, input)),
        None => Arc::new(CoalescePartitionsExec::new(input)),
      };
    }
    let sort_order = input
      .properties()
      .output_ordering()
      .cloned()
      .map(Into::into);
    let sink = create_sink(
      &state,
      write_path,
      input.schema(),
      Vec::new(),
      writer_options,
      Arc::clone(&self.tracker),
    )?;
    let plan: Arc<dyn ExecutionPlan> =
      Arc::new(DataSinkExec::new(input, Arc::clone(&sink) as _, sort_order));
    let rows_written = execute_sink_plan(plan, sink, context).await?;
    self.tracker.finish(rows_written);
    Ok(rows_written)
  }

  /// Write one DataFrame through concurrent partition-local Parquet sinks.
  pub(crate) async fn write_partitioned<RewritePlan>(
    self,
    dataframe: DataFrame,
    write_path: String,
    partition_by: Vec<String>,
    writer_options: TableParquetOptions,
    rewrite_plan: RewritePlan,
  ) -> Result<u64>
  where
    RewritePlan: FnOnce(Arc<dyn ExecutionPlan>) -> DataFusionResult<Arc<dyn ExecutionPlan>>,
  {
    let (state, logical_plan) = dataframe.into_parts();
    let context = Arc::new(TaskContext::from(&state));
    let input = rewrite_plan(state.create_physical_plan(&logical_plan).await?)?;
    let sink = create_sink(
      &state,
      write_path,
      input.schema(),
      partition_by,
      writer_options,
      Arc::clone(&self.tracker),
    )?;
    let plan: Arc<dyn ExecutionPlan> =
      Arc::new(ConcurrentPartitionSinkExec::new(input, Arc::clone(&sink)));
    let rows_written = execute_sink_plan(plan, sink, context).await?;
    self.tracker.finish(rows_written);
    Ok(rows_written)
  }
}

fn create_sink(
  state: &datafusion::execution::context::SessionState,
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
    keep_partition_by_columns: state.config_options().execution.keep_partition_by_columns,
    file_extension: "parquet".to_string(),
  };
  Ok(Arc::new(TrackingParquetSink::new(
    config,
    writer_options,
    tracker,
  )))
}

async fn execute_sink_plan(
  plan: Arc<dyn ExecutionPlan>,
  sink: Arc<TrackingParquetSink>,
  context: Arc<TaskContext>,
) -> Result<u64> {
  let batches = match collect(plan, Arc::clone(&context)).await {
    Ok(batches) => batches,
    Err(error) => {
      if let Err(cleanup_error) = sink.cleanup_written_files(&context).await {
        return Err(anyhow!(
          "parquet write failed: {error}; cleanup failed: {cleanup_error}"
        ));
      }
      return Err(error.into());
    }
  };
  let batch = batches.first().context("write returned no row count")?;
  let values = batch
    .column(0)
    .as_any()
    .downcast_ref::<UInt64Array>()
    .context("write result count column was not UInt64")?;
  Ok(if values.is_empty() {
    0
  } else {
    values.value(0)
  })
}

#[derive(Clone, Debug)]
struct ConcurrentPartitionSinkExec {
  input: Arc<dyn ExecutionPlan>,
  sink: Arc<TrackingParquetSink>,
  count_schema: SchemaRef,
  cache: PlanProperties,
}

impl ConcurrentPartitionSinkExec {
  fn new(input: Arc<dyn ExecutionPlan>, sink: Arc<TrackingParquetSink>) -> Self {
    let count_schema = count_schema();
    let cache = PlanProperties::new(
      EquivalenceProperties::new(Arc::clone(&count_schema)),
      Partitioning::UnknownPartitioning(1),
      input.pipeline_behavior(),
      input.boundedness(),
    )
    .with_scheduling_type(SchedulingType::Cooperative)
    .with_evaluation_type(EvaluationType::Eager);
    Self {
      input,
      sink,
      count_schema,
      cache,
    }
  }
}

impl DisplayAs for ConcurrentPartitionSinkExec {
  fn fmt_as(
    &self,
    format_type: DisplayFormatType,
    formatter: &mut fmt::Formatter<'_>,
  ) -> fmt::Result {
    self.sink.fmt_as(format_type, formatter)
  }
}

impl ExecutionPlan for ConcurrentPartitionSinkExec {
  fn name(&self) -> &'static str {
    "ConcurrentPartitionSinkExec"
  }

  fn as_any(&self) -> &dyn Any {
    self
  }

  fn properties(&self) -> &PlanProperties {
    &self.cache
  }

  fn required_input_distribution(&self) -> Vec<Distribution> {
    vec![Distribution::UnspecifiedDistribution]
  }

  fn maintains_input_order(&self) -> Vec<bool> {
    vec![true]
  }

  fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
    vec![&self.input]
  }

  fn with_new_children(
    self: Arc<Self>,
    children: Vec<Arc<dyn ExecutionPlan>>,
  ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
    Ok(Arc::new(Self::new(
      Arc::clone(&children[0]),
      Arc::clone(&self.sink),
    )))
  }

  fn execute(
    &self,
    partition: usize,
    context: Arc<TaskContext>,
  ) -> DataFusionResult<SendableRecordBatchStream> {
    if partition != 0 {
      return Err(DataFusionError::Execution(format!(
        "{} can only execute partition 0",
        self.name()
      )));
    }
    let count_schema = Arc::clone(&self.count_schema);
    let input = Arc::clone(&self.input);
    let sink = Arc::clone(&self.sink);
    let stream = futures_util::stream::once(async move {
      run_concurrent_partition_writes(input, sink, &context)
        .await
        .map(make_count_batch)
    });
    Ok(Box::pin(RecordBatchStreamAdapter::new(
      count_schema,
      stream,
    )))
  }
}

async fn run_concurrent_partition_writes(
  input: Arc<dyn ExecutionPlan>,
  sink: Arc<TrackingParquetSink>,
  context: &Arc<TaskContext>,
) -> DataFusionResult<u64> {
  let mut write_tasks = JoinSet::new();
  for partition in 0..input.output_partitioning().partition_count() {
    let input = Arc::clone(&input);
    let sink = Arc::clone(&sink);
    let context = Arc::clone(context);
    write_tasks.spawn(async move {
      let data = execute_input_stream(
        input,
        Arc::clone(sink.schema()),
        partition,
        Arc::clone(&context),
      )?;
      sink.write_all(data, &context).await
    });
  }

  let mut rows_written = 0;
  let mut first_error = None;
  while let Some(result) = write_tasks.join_next().await {
    match result {
      Ok(Ok(count)) => rows_written += count,
      Ok(Err(error)) => {
        first_error.get_or_insert(error);
      }
      Err(error) if error.is_panic() => std::panic::resume_unwind(error.into_panic()),
      Err(error) => {
        first_error.get_or_insert_with(|| {
          DataFusionError::Execution(format!("partitioned parquet write task failed: {error}"))
        });
      }
    }
  }
  first_error.map_or(Ok(rows_written), Err)
}

fn count_schema() -> SchemaRef {
  Arc::new(Schema::new(vec![Field::new(
    "count",
    DataType::UInt64,
    false,
  )]))
}

fn make_count_batch(count: u64) -> RecordBatch {
  RecordBatch::try_new(
    count_schema(),
    vec![Arc::new(UInt64Array::from(vec![count]))],
  )
  .expect("count batch should always be valid")
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

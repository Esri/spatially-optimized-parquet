//! Executes partitioned Parquet output with live metrics and failed-write cleanup.

use std::any::Any;
use std::fmt;
use std::sync::{
  Arc,
  atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use anyhow::Result;
use arrow_array::{RecordBatch, UInt64Array};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use async_trait::async_trait;
use datafusion::common::{
  DataFusionError, Result as DataFusionResult, config::TableParquetOptions,
};
use datafusion::dataframe::DataFrame;
use datafusion::datasource::file_format::parquet::ParquetSink;
use datafusion::datasource::listing::ListingTableUrl;
use datafusion::datasource::physical_plan::FileSinkConfig;
use datafusion::datasource::sink::DataSink;
use datafusion::execution::TaskContext;
use datafusion::logical_expr::dml::InsertOp;
use datafusion::physical_expr::{Distribution, EquivalenceProperties, LexRequirement};
use datafusion::physical_plan::{
  DisplayAs, DisplayFormatType, ExecutionPlan, ExecutionPlanProperties, Partitioning,
  PlanProperties, SendableRecordBatchStream, collect, execute_input_stream,
  execution_plan::{EvaluationType, SchedulingType},
  metrics::{Count, ExecutionPlanMetricsSet, MetricBuilder, MetricsSet},
  stream::RecordBatchStreamAdapter,
};
use engine::written_row_count;
use futures_util::StreamExt;
use indicatif::ProgressBar;
use tokio::task::JoinSet;

use crate::diagnostics::{
  explain_dataframe_verbose, explain_physical_plan, explain_stage_completion,
};
use crate::progress::{SINK_ROWS_METRIC, collect_plan_progress, update_write_stage_bar};

use super::partitioned_sort::PartitionedSortConfig;

#[derive(Debug)]
/// Wraps DataFusion's Parquet sink with row metrics and failed-write cleanup support.
struct TrackingParquetSink {
  config: FileSinkConfig,
  inner: ParquetSink,
  metrics: ExecutionPlanMetricsSet,
  sink_rows: Count,
}

impl TrackingParquetSink {
  fn new(config: FileSinkConfig, parquet_options: TableParquetOptions) -> Self {
    let metrics = ExecutionPlanMetricsSet::new();
    let sink_rows = MetricBuilder::new(&metrics).global_counter(SINK_ROWS_METRIC);
    Self {
      config: config.clone(),
      inner: ParquetSink::new(config, parquet_options),
      metrics,
      sink_rows,
    }
  }

  async fn cleanup_written_files(&self, context: &Arc<TaskContext>) -> DataFusionResult<()> {
    let written_files = self.inner.written();
    if written_files.is_empty() {
      return Ok(());
    }

    let object_store = context
      .runtime_env()
      .object_store(&self.config.object_store_url)?;
    let mut cleanup_error = None;
    for path in written_files.keys() {
      if let Err(error) = object_store.delete(path).await
        && cleanup_error.is_none()
      {
        cleanup_error = Some(DataFusionError::ObjectStore(Box::new(error)));
      }
    }

    cleanup_error.map_or(Ok(()), Err)
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

  fn metrics(&self) -> Option<MetricsSet> {
    Some(self.metrics.clone_inner())
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
    let sink_rows = self.sink_rows.clone();
    let tracked_stream = data.map(move |batch| {
      if let Ok(batch) = &batch {
        sink_rows.add(batch.num_rows());
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

#[derive(Clone, Debug)]
/// Executes every sorted input partition as a concurrent Parquet write task.
struct ConcurrentPartitionedParquetSinkExec {
  input: Arc<dyn ExecutionPlan>,
  sink: Arc<TrackingParquetSink>,
  count_schema: SchemaRef,
  sort_order: Option<LexRequirement>,
  cache: PlanProperties,
}

impl ConcurrentPartitionedParquetSinkExec {
  fn new(
    input: Arc<dyn ExecutionPlan>,
    sink: Arc<TrackingParquetSink>,
    sort_order: Option<LexRequirement>,
  ) -> Self {
    let count_schema = count_schema();
    let equivalence_properties = EquivalenceProperties::new(Arc::clone(&count_schema));
    let cache = PlanProperties::new(
      equivalence_properties,
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
      sort_order,
      cache,
    }
  }
}

impl DisplayAs for ConcurrentPartitionedParquetSinkExec {
  fn fmt_as(
    &self,
    format_type: DisplayFormatType,
    formatter: &mut fmt::Formatter<'_>,
  ) -> fmt::Result {
    match format_type {
      DisplayFormatType::Default | DisplayFormatType::Verbose => {
        write!(formatter, "ConcurrentPartitionedParquetSinkExec: sink=")?;
        self.sink.fmt_as(format_type, formatter)
      }
      DisplayFormatType::TreeRender => self.sink.fmt_as(format_type, formatter),
    }
  }
}

impl ExecutionPlan for ConcurrentPartitionedParquetSinkExec {
  fn name(&self) -> &'static str {
    "ConcurrentPartitionedParquetSinkExec"
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

  fn required_input_ordering(
    &self,
  ) -> Vec<Option<datafusion::physical_expr::OrderingRequirements>> {
    vec![self.sort_order.as_ref().cloned().map(Into::into)]
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
      self.sort_order.clone(),
    )))
  }

  fn execute(
    &self,
    partition: usize,
    context: Arc<TaskContext>,
  ) -> DataFusionResult<SendableRecordBatchStream> {
    if partition != 0 {
      return Err(DataFusionError::Execution(format!(
        "{} can only be called on partition 0",
        self.name()
      )));
    }

    let count_schema = Arc::clone(&self.count_schema);
    let input = Arc::clone(&self.input);
    let sink = Arc::clone(&self.sink);
    let stream = futures_util::stream::once(async move {
      run_concurrent_partitioned_parquet_writes(input, sink, &context)
        .await
        .map(make_count_batch)
    });
    Ok(Box::pin(RecordBatchStreamAdapter::new(
      count_schema,
      stream,
    )))
  }

  fn metrics(&self) -> Option<MetricsSet> {
    self.sink.metrics()
  }
}

async fn run_concurrent_partitioned_parquet_writes(
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
        Arc::clone(&input),
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
    };
  }

  if let Some(error) = first_error {
    if let Err(cleanup_error) = sink.cleanup_written_files(context).await {
      return Err(DataFusionError::Execution(format!(
        "partitioned parquet write failed: {error}; cleanup failed: {cleanup_error}"
      )));
    }
    return Err(error);
  }
  Ok(rows_written)
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

pub(super) struct PartitionedParquetWriter<'a> {
  write_path: String,
  partition_by: Vec<String>,
  partitioned_sort: PartitionedSortConfig,
  writer_options: TableParquetOptions,
  progress_bar: &'a ProgressBar,
  total_input_rows: u64,
  explain: bool,
}

impl<'a> PartitionedParquetWriter<'a> {
  /// Construct one partitioned Parquet sink execution.
  pub(super) fn new(
    write_path: String,
    partition_by: Vec<String>,
    partitioned_sort: PartitionedSortConfig,
    writer_options: TableParquetOptions,
    progress_bar: &'a ProgressBar,
    total_input_rows: u64,
    explain: bool,
  ) -> Self {
    Self {
      write_path,
      partition_by,
      partitioned_sort,
      writer_options,
      progress_bar,
      total_input_rows,
      explain,
    }
  }

  /// Write one prepared DataFrame through the partitioned Parquet sink.
  pub(super) async fn write(self, dataframe: DataFrame) -> Result<u64> {
    let (state, logical_plan) = dataframe.into_parts();
    explain_dataframe_verbose(
      self.explain,
      "Writing parquet output input query",
      &state,
      &logical_plan,
      false,
    )
    .await?;
    let task_context = Arc::new(TaskContext::from(&state));
    let input_plan = state.create_physical_plan(&logical_plan).await?;
    let rewritten_input = self
      .partitioned_sort
      .insert_into(input_plan)
      .map_err(anyhow::Error::from)?;
    let parsed_url = ListingTableUrl::parse(&self.write_path)?;
    let sink_config = FileSinkConfig {
      original_url: self.write_path.clone(),
      object_store_url: parsed_url.object_store(),
      file_group: Default::default(),
      table_paths: vec![parsed_url],
      output_schema: rewritten_input.schema(),
      table_partition_cols: self
        .partition_by
        .iter()
        .map(|column| (column.to_string(), DataType::Null))
        .collect(),
      insert_op: InsertOp::Append,
      keep_partition_by_columns: state.config_options().execution.keep_partition_by_columns,
      file_extension: "parquet".to_string(),
    };
    let sink = Arc::new(TrackingParquetSink::new(sink_config, self.writer_options));
    let physical_plan: Arc<dyn ExecutionPlan> = Arc::new(
      ConcurrentPartitionedParquetSinkExec::new(rewritten_input, sink, None),
    );
    explain_physical_plan(self.explain, "Writing parquet output", &physical_plan);
    let stage_start = Instant::now();
    let total_input_rows = self.total_input_rows;

    let done = Arc::new(AtomicBool::new(false));
    let poller = if self.progress_bar.is_hidden() {
      None
    } else {
      let plan = Arc::clone(&physical_plan);
      let bar = self.progress_bar.clone();
      let done = Arc::clone(&done);
      Some(std::thread::spawn(move || {
        let mut current_phase = None;
        while !done.load(Ordering::Relaxed) {
          update_write_stage_bar(
            &bar,
            collect_plan_progress(plan.as_ref()),
            total_input_rows,
            true,
            &mut current_phase,
          );
          std::thread::sleep(Duration::from_millis(500));
        }
      }))
    };

    let result = collect(Arc::clone(&physical_plan), task_context).await;
    done.store(true, Ordering::Relaxed);
    if let Some(poller) = poller {
      let _ = poller.join();
    }
    explain_stage_completion(
      self.explain,
      "Writing parquet output",
      stage_start.elapsed(),
      &physical_plan,
      collect_plan_progress(physical_plan.as_ref()),
    );
    written_row_count(&result?)
  }
}

//! Provides the Parquet output-writing facade.

use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use arrow_array::{Array, UInt64Array};
use datafusion::common::{Result as DataFusionResult, config::TableParquetOptions};
use datafusion::dataframe::DataFrame;
use datafusion::datasource::sink::DataSinkExec;
use datafusion::execution::TaskContext;
use datafusion::physical_expr::expressions::Column as PhysicalColumn;
use datafusion::physical_plan::{
  ExecutionPlan, ExecutionPlanProperties,
  coalesce_partitions::CoalescePartitionsExec,
  collect,
  projection::{ProjectionExec, ProjectionExpr},
  sorts::sort_preserving_merge::SortPreservingMergeExec,
};

use crate::pipeline::SharedWriteReporter;
use crate::plan_diagnostics::print_physical_plan;

use super::parquet_partition_exec::ConcurrentPartitionSinkExec;
use super::parquet_sink::{TrackingParquetSink, WriteTracker};

/// Executes single-file and concurrent partitioned Parquet writes through one tracking sink.
pub(crate) struct ParquetOutputWriter {
  tracker: Arc<WriteTracker>,
}

impl ParquetOutputWriter {
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
    hidden_columns: Vec<&str>,
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
    let sort_order = if hidden_columns.is_empty() {
      input
        .properties()
        .output_ordering()
        .cloned()
        .map(Into::into)
    } else {
      input = Self::project_without_columns(input, &hidden_columns)?;
      None
    };
    let sink = TrackingParquetSink::create(
      write_path,
      input.schema(),
      Vec::new(),
      writer_options,
      Arc::clone(&self.tracker),
    )?;
    let plan: Arc<dyn ExecutionPlan> =
      Arc::new(DataSinkExec::new(input, Arc::clone(&sink) as _, sort_order));
    let rows_written = Self::execute_sink_plan("single-file sink", plan, sink, context).await?;
    self.tracker.finish(rows_written);
    Ok(rows_written)
  }

  fn project_without_columns(
    input: Arc<dyn ExecutionPlan>,
    hidden_columns: &[&str],
  ) -> Result<Arc<dyn ExecutionPlan>> {
    let expressions = input
      .schema()
      .fields()
      .iter()
      .enumerate()
      .filter(|(_, field)| !hidden_columns.contains(&field.name().as_str()))
      .map(|(index, field)| ProjectionExpr {
        expr: Arc::new(PhysicalColumn::new(field.name(), index)),
        alias: field.name().to_string(),
      })
      .collect::<Vec<_>>();
    Ok(Arc::new(ProjectionExec::try_new(expressions, input)?))
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
    let sink = TrackingParquetSink::create(
      write_path,
      input.schema(),
      partition_by,
      writer_options,
      Arc::clone(&self.tracker),
    )?;
    let plan: Arc<dyn ExecutionPlan> =
      Arc::new(ConcurrentPartitionSinkExec::new(input, Arc::clone(&sink)));
    let rows_written = Self::execute_sink_plan("partitioned sink", plan, sink, context).await?;
    self.tracker.finish(rows_written);
    Ok(rows_written)
  }

  async fn execute_sink_plan(
    label: &str,
    plan: Arc<dyn ExecutionPlan>,
    sink: Arc<TrackingParquetSink>,
    context: Arc<TaskContext>,
  ) -> Result<u64> {
    print_physical_plan(label, plan.as_ref());
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
}

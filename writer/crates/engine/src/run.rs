//! Streams DataFusion output through a caller-provided batch transform into direct Parquet writers.
//!
//! [`run_df`] consumes physical partition streams and favors throughput over global order.
//! [`run_ordered_df`] consumes one logical stream so rows reach output writers in plan order.
//! Both paths apply the same transform contract, advance between files using an approximate
//! row target, emit [`RunStatus`] lifecycle events, attach file metadata, and close every writer.
//!
//! This module supports the legacy direct-writer architecture. The current spatial job normally
//! delegates sorting and final output to DataFusion sinks, which can spill and write partitions
//! concurrently. The direct path remains useful where callers already own batch transformation
//! and do not need the custom physical-plan wrappers in `spatial::job`.

use anyhow::Result;
use arrow_array::RecordBatch;
use arrow_schema::SchemaRef;
use datafusion::dataframe::DataFrame;
use futures_util::StreamExt;
use parquet::basic::Compression;
use parquet::file::metadata::KeyValue;

use crate::plan::{OutputPlan, output_paths, target_rows_per_file};
use crate::read::{execute_partitioned, execute_stream};
use crate::write::{OutputWriter, create_output_writer, finalize_writers, write_batches};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Describes observable lifecycle transitions during a streaming write.
pub enum RunStatus {
  /// Indicates that execution is awaiting the next source batch.
  WaitingForBatch,
  /// Indicates that the configured transform is processing a batch.
  TransformingBatch {
    /// Stores the batch row count.
    rows: usize,
  },
  /// Indicates that a transformed batch is entering one output writer.
  WritingBatch {
    /// Stores the batch row count.
    rows: usize,
    /// Stores the one-based destination file index.
    file_index: usize,
    /// Stores the total destination file count.
    file_count: usize,
  },
  /// Indicates that one batch write completed.
  BatchWritten {
    /// Stores the batch row count.
    rows: usize,
    /// Stores the one-based destination file index.
    file_index: usize,
    /// Stores the total destination file count.
    file_count: usize,
  },
  /// Indicates that file metadata is being attached and writers are closing.
  FinalizingOutputs,
}

/// Configures batch transformation, output layout, metadata, and progress reporting.
pub struct RunConfig<'a, F>
where
  F: Fn(&RecordBatch, &SchemaRef) -> Result<RecordBatch> + Send + Sync,
{
  /// Supplies the schema expected after `transform` runs.
  pub output_schema: &'a SchemaRef,
  /// Supplies the validated destination layout.
  pub output_plan: &'a OutputPlan,
  /// Selects the Parquet compression codec.
  pub compression: Compression,
  /// Supplies the estimated total row count used to divide files.
  pub total_rows: u64,
  /// Supplies file-level Parquet key-value metadata.
  pub kv_metadata: &'a [KeyValue],
  /// Converts each source batch into the output schema.
  pub transform: F,
  /// Receives synchronous lifecycle notifications when configured.
  pub on_status: Option<&'a dyn Fn(RunStatus)>,
}

/// Execute physical partitions and distribute transformed batches across output files.
///
/// Partition streams may arrive in reverse pop order, so this path does not promise
/// global row ordering.
pub async fn run_df<F>(df: DataFrame, config: RunConfig<'_, F>) -> Result<()>
where
  F: Fn(&RecordBatch, &SchemaRef) -> Result<RecordBatch> + Send + Sync,
{
  let mut writers = build_writers(config.output_plan, config.output_schema, config.compression)?;
  let mut streams = execute_partitioned(df).await?;
  let mut writer_idx = 0usize;
  let target_rows = target_rows_per_file(config.output_plan.parts, config.total_rows);
  let file_count = writers.len();

  while let Some(stream) = streams.pop() {
    let mut stream = stream;
    loop {
      report_status(config.on_status, RunStatus::WaitingForBatch);
      let Some(batch) = stream.next().await else {
        break;
      };
      let batch = batch?;
      report_status(
        config.on_status,
        RunStatus::TransformingBatch {
          rows: batch.num_rows(),
        },
      );
      let batch = (config.transform)(&batch, config.output_schema)?;
      let file_index = writer_idx + 1;
      report_status(
        config.on_status,
        RunStatus::WritingBatch {
          rows: batch.num_rows(),
          file_index,
          file_count,
        },
      );
      let writer = &mut writers[writer_idx];
      write_batches(writer, &batch)?;
      report_status(
        config.on_status,
        RunStatus::BatchWritten {
          rows: batch.num_rows(),
          file_index,
          file_count,
        },
      );
      if config.output_plan.parts > 1 && target_rows > 0 && writer.rows_written >= target_rows {
        writer_idx = (writer_idx + 1).min(writers.len() - 1);
      }
    }
  }

  report_status(config.on_status, RunStatus::FinalizingOutputs);
  finalize_writers(writers, config.kv_metadata)?;
  Ok(())
}

/// Execute one ordered stream and distribute transformed batches without reordering rows.
pub async fn run_ordered_df<F>(df: DataFrame, config: RunConfig<'_, F>) -> Result<()>
where
  F: Fn(&RecordBatch, &SchemaRef) -> Result<RecordBatch> + Send + Sync,
{
  let mut writers = build_writers(config.output_plan, config.output_schema, config.compression)?;
  let mut stream = execute_stream(df).await?;
  let mut writer_idx = 0usize;
  let target_rows = target_rows_per_file(config.output_plan.parts, config.total_rows);
  let file_count = writers.len();

  loop {
    report_status(config.on_status, RunStatus::WaitingForBatch);
    let Some(batch) = stream.next().await else {
      break;
    };
    let batch = batch?;
    report_status(
      config.on_status,
      RunStatus::TransformingBatch {
        rows: batch.num_rows(),
      },
    );
    let batch = (config.transform)(&batch, config.output_schema)?;
    let file_index = writer_idx + 1;
    report_status(
      config.on_status,
      RunStatus::WritingBatch {
        rows: batch.num_rows(),
        file_index,
        file_count,
      },
    );
    let writer = &mut writers[writer_idx];
    write_batches(writer, &batch)?;
    report_status(
      config.on_status,
      RunStatus::BatchWritten {
        rows: batch.num_rows(),
        file_index,
        file_count,
      },
    );
    if config.output_plan.parts > 1 && target_rows > 0 && writer.rows_written >= target_rows {
      writer_idx = (writer_idx + 1).min(writers.len() - 1);
    }
  }

  report_status(config.on_status, RunStatus::FinalizingOutputs);
  finalize_writers(writers, config.kv_metadata)?;
  Ok(())
}

fn report_status(callback: Option<&dyn Fn(RunStatus)>, status: RunStatus) {
  if let Some(callback) = callback {
    callback(status);
  }
}

/// Create one direct Parquet writer for every resolved output path.
fn build_writers(
  output_plan: &OutputPlan,
  schema: &SchemaRef,
  compression: Compression,
) -> Result<Vec<OutputWriter>> {
  let output_paths = output_paths(output_plan)?;
  output_paths
    .iter()
    .map(|path| create_output_writer(path, schema, compression))
    .collect::<Result<Vec<_>>>()
}

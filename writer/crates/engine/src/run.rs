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
pub enum RunStatus {
  WaitingForBatch,
  TransformingBatch {
    rows: usize,
  },
  WritingBatch {
    rows: usize,
    file_index: usize,
    file_count: usize,
  },
  BatchWritten {
    rows: usize,
    file_index: usize,
    file_count: usize,
  },
  FinalizingOutputs,
}

pub struct RunConfig<'a, F>
where
  F: Fn(&RecordBatch, &SchemaRef) -> Result<RecordBatch> + Send + Sync,
{
  pub output_schema: &'a SchemaRef,
  pub output_plan: &'a OutputPlan,
  pub compression: Compression,
  pub total_rows: u64,
  pub kv_metadata: &'a [KeyValue],
  pub transform: F,
  pub on_status: Option<&'a dyn Fn(RunStatus)>,
}

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

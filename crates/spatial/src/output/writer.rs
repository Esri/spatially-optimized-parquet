//! Provides the Parquet output-writing facade.

use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use arrow_array::{Array, UInt64Array};
use datafusion::common::{
  Result as DataFusionResult,
  config::{ParquetColumnOptions, TableParquetOptions},
  parquet_config::DFParquetWriterVersion,
};
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
use parquet::basic::{BrotliLevel, Compression, GzipLevel, ZstdLevel};
use parquet::file::metadata::KeyValue;

use crate::pipeline::SharedWriteReporter;
use crate::plan_diagnostics::print_physical_plan;

use super::partition_plan::PartitionPlan;
use super::reporter::WriteReporter;
use super::tracking_sink::TrackingSink;

const DEFAULT_MAX_ROW_GROUP_SIZE: usize = 128 * 1024;
const DEFAULT_WRITE_BATCH_SIZE: usize = 8 * 1024;
const ROW_GROUP_SIZE_ENV: &str = "OPT_PARQUET_ROW_GROUP_SIZE";
const WRITE_BATCH_SIZE_ENV: &str = "OPT_PARQUET_WRITE_BATCH_SIZE";

/// Owns the configured DataFusion options for one Parquet write.
pub(crate) struct WriterOptions {
  options: TableParquetOptions,
}

impl WriterOptions {
  /// Build Parquet writer options from a compression name and key-value metadata.
  pub(crate) fn new(compression: &str, kv_metadata: &[KeyValue]) -> Result<Self> {
    let compression = Self::parse_compression(compression)?;
    let mut options = TableParquetOptions::new();
    options.global.compression = Some(Self::datafusion_compression_name(compression));
    options.global.dictionary_enabled = Some(true);
    options.global.writer_version = DFParquetWriterVersion::V2_0;
    options.global.maximum_parallel_row_group_writers = Self::available_parallelism();
    options.global.max_row_group_size = Self::configured_max_row_group_size();
    options.global.write_batch_size = Self::configured_write_batch_size();
    options.key_value_metadata = kv_metadata
      .iter()
      .map(|kv| (kv.key.clone(), kv.value.clone()))
      .collect();
    Ok(Self { options })
  }

  /// Consume the typed options for a custom DataFusion Parquet sink.
  fn into_datafusion(self) -> TableParquetOptions {
    self.options
  }

  /// Apply integer delta packing to selected physical coordinate leaves.
  pub(crate) fn with_delta_binary_packed_columns(
    mut self,
    columns: impl IntoIterator<Item = String>,
  ) -> Self {
    for column in columns {
      self.options.column_specific_options.insert(
        column,
        ParquetColumnOptions {
          encoding: Some("delta_binary_packed".to_string()),
          dictionary_enabled: Some(false),
          ..Default::default()
        },
      );
    }
    self
  }

  fn parse_compression(compression: &str) -> Result<Compression> {
    let codec = match compression.to_ascii_lowercase().as_str() {
      "snappy" => Compression::SNAPPY,
      "gzip" => Compression::GZIP(GzipLevel::default()),
      "brotli" => Compression::BROTLI(BrotliLevel::default()),
      "lz4" => Compression::LZ4,
      "lz4_raw" => Compression::LZ4_RAW,
      "zstd" => Compression::ZSTD(ZstdLevel::default()),
      "uncompressed" => Compression::UNCOMPRESSED,
      other => {
        return Err(std::io::Error::new(
          std::io::ErrorKind::InvalidInput,
          format!("invalid compression: {other}"),
        ))
        .context("parse compression");
      }
    };
    Ok(codec)
  }

  fn datafusion_compression_name(compression: Compression) -> String {
    match compression {
      Compression::UNCOMPRESSED => "uncompressed".to_string(),
      Compression::SNAPPY => "snappy".to_string(),
      Compression::GZIP(level) => format!("gzip({})", level.compression_level()),
      Compression::LZO => "lzo".to_string(),
      Compression::BROTLI(level) => format!("brotli({})", level.compression_level()),
      Compression::LZ4 => "lz4".to_string(),
      Compression::ZSTD(level) => format!("zstd({})", level.compression_level()),
      Compression::LZ4_RAW => "lz4_raw".to_string(),
    }
  }

  fn configured_max_row_group_size() -> usize {
    Self::env_usize(ROW_GROUP_SIZE_ENV).unwrap_or(DEFAULT_MAX_ROW_GROUP_SIZE)
  }

  fn configured_write_batch_size() -> usize {
    Self::env_usize(WRITE_BATCH_SIZE_ENV).unwrap_or(DEFAULT_WRITE_BATCH_SIZE)
  }

  fn available_parallelism() -> usize {
    std::thread::available_parallelism()
      .map(usize::from)
      .unwrap_or(1)
  }

  fn env_usize(name: &str) -> Option<usize> {
    std::env::var(name)
      .ok()
      .and_then(|value| value.parse::<usize>().ok())
      .filter(|value| *value > 0)
  }
}

/// Executes single-file and partitioned Parquet writes through one tracking sink.
pub(crate) struct Writer {
  reporter: Arc<WriteReporter>,
}

impl Writer {
  /// Construct one writer with a shared cumulative row counter.
  pub(crate) fn new(total_rows: u64, reporter: Option<SharedWriteReporter>) -> Self {
    Self {
      reporter: Arc::new(WriteReporter::new(total_rows, reporter)),
    }
  }

  /// Write one DataFrame into an exact Parquet file path.
  pub(crate) async fn write_single(
    self,
    dataframe: DataFrame,
    write_path: String,
    writer_options: WriterOptions,
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
    let sink = TrackingSink::create(
      write_path,
      input.schema(),
      Vec::new(),
      writer_options.into_datafusion(),
      Arc::clone(&self.reporter),
    )?;
    let plan: Arc<dyn ExecutionPlan> =
      Arc::new(DataSinkExec::new(input, Arc::clone(&sink) as _, sort_order));
    let rows_written = Self::execute_sink_plan("single-file sink", plan, sink, context).await?;
    self.reporter.finish(rows_written);
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

  /// Write one DataFrame through partition-local Parquet sinks.
  pub(crate) async fn write_partitioned<RewritePlan>(
    self,
    dataframe: DataFrame,
    write_path: String,
    partition_by: Vec<String>,
    writer_options: WriterOptions,
    rewrite_plan: RewritePlan,
  ) -> Result<u64>
  where
    RewritePlan: FnOnce(Arc<dyn ExecutionPlan>) -> DataFusionResult<Arc<dyn ExecutionPlan>>,
  {
    let (state, logical_plan) = dataframe.into_parts();
    let context = Arc::new(TaskContext::from(&state));
    let input = rewrite_plan(state.create_physical_plan(&logical_plan).await?)?;
    let sink = TrackingSink::create(
      write_path,
      input.schema(),
      partition_by,
      writer_options.into_datafusion(),
      Arc::clone(&self.reporter),
    )?;
    let plan: Arc<dyn ExecutionPlan> = Arc::new(PartitionPlan::new(input, Arc::clone(&sink)));
    let rows_written = Self::execute_sink_plan("partitioned sink", plan, sink, context).await?;
    self.reporter.finish(rows_written);
    Ok(rows_written)
  }

  async fn execute_sink_plan(
    label: &str,
    plan: Arc<dyn ExecutionPlan>,
    sink: Arc<TrackingSink>,
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

#[cfg(test)]
mod tests {
  use super::WriterOptions;

  #[test]
  fn compression_parser_rejects_invalid_codec() {
    let error = WriterOptions::parse_compression("bogus").unwrap_err();
    assert!(error.to_string().contains("compression"));
  }
}

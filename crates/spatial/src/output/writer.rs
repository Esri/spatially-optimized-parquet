//! Provides the Parquet output-writing facade.

use std::sync::Arc;

use arrow_array::{Array, UInt64Array};
use arrow_schema::Schema;
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
use parquet::arrow::ArrowSchemaConverter;
use parquet::basic::LogicalType;
use parquet::basic::{BrotliLevel, Compression, GzipLevel, ZstdLevel};
use parquet::file::metadata::KeyValue;

use crate::diagnostics::Diagnostics;
use crate::pipeline::SharedWriteReporter;

use super::OutputError;
use super::geometry_schema::GeometrySchemaExec;
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
  geometry_column: Option<String>,
  geometry_crs: Option<String>,
}

impl WriterOptions {
  /// Build Parquet writer options from a compression name and key-value metadata.
  pub(crate) fn new(compression: &str, kv_metadata: &[KeyValue]) -> Result<Self, OutputError> {
    let compression = Self::parse_compression(compression)?;
    let mut options = TableParquetOptions::new();
    options.global.compression = Some(Self::datafusion_compression_name(compression));
    options.global.dictionary_enabled = Some(false);
    options.global.writer_version = DFParquetWriterVersion::V2_0;
    options.global.maximum_parallel_row_group_writers = Self::available_parallelism();
    options.global.max_row_group_size = Self::configured_max_row_group_size();
    options.global.write_batch_size = Self::configured_write_batch_size();
    options.key_value_metadata = kv_metadata
      .iter()
      .map(|kv| (kv.key.clone(), kv.value.clone()))
      .collect();
    Ok(Self {
      options,
      geometry_column: None,
      geometry_crs: None,
    })
  }

  /// Configure the primary WKB column as a native Parquet GEOMETRY type.
  pub(crate) fn with_geometry_column(mut self, column: &str, crs: String) -> Self {
    self.geometry_column = Some(column.to_string());
    self.geometry_crs = Some(crs);
    self
  }

  /// Enable adaptive dictionary encoding for physical string leaves.
  fn into_table_options(mut self, schema: &Schema) -> DataFusionResult<TableParquetOptions> {
    let parquet_schema = ArrowSchemaConverter::new().convert(schema)?;
    for column in parquet_schema.columns() {
      if matches!(column.logical_type_ref(), Some(LogicalType::String)) {
        self
          .options
          .column_specific_options
          .entry(column.path().string())
          .or_default()
          .dictionary_enabled = Some(true);
      }
    }
    Ok(self.options)
  }

  fn apply_geometry_schema(
    &self,
    input: Arc<dyn ExecutionPlan>,
  ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
    match (&self.geometry_column, &self.geometry_crs) {
      (Some(column), Some(crs)) => Ok(Arc::new(GeometrySchemaExec::try_new(input, column, crs)?)),
      (None, None) => Ok(input),
      _ => Err(datafusion::common::DataFusionError::Plan(
        "geometry column and CRS must be configured together".to_string(),
      )),
    }
  }

  /// Apply byte-stream splitting to selected floating-point coordinate leaves.
  pub(crate) fn with_byte_stream_split_columns(
    mut self,
    columns: impl IntoIterator<Item = String>,
  ) -> Self {
    for column in columns {
      self.options.column_specific_options.insert(
        column,
        ParquetColumnOptions {
          encoding: Some("byte_stream_split".to_string()),
          dictionary_enabled: Some(false),
          ..Default::default()
        },
      );
    }
    self
  }

  fn parse_compression(compression: &str) -> Result<Compression, OutputError> {
    let codec = match compression.to_ascii_lowercase().as_str() {
      "snappy" => Compression::SNAPPY,
      "gzip" => Compression::GZIP(GzipLevel::default()),
      "brotli" => Compression::BROTLI(BrotliLevel::default()),
      "lz4" => Compression::LZ4,
      "lz4_raw" => Compression::LZ4_RAW,
      "zstd" => Compression::ZSTD(ZstdLevel::default()),
      "uncompressed" => Compression::UNCOMPRESSED,
      other => {
        return Err(OutputError::Configuration(format!(
          "invalid compression: {other}"
        )));
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
  ) -> Result<u64, OutputError> {
    let (state, logical_plan) = dataframe.into_parts();
    let context = Arc::new(TaskContext::from(&state));
    let mut input = state
      .create_physical_plan(&logical_plan)
      .await
      .map_err(|source| OutputError::DataFusion {
        operation: "create single-file physical plan",
        source,
      })?;
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
    input = writer_options
      .apply_geometry_schema(input)
      .map_err(|source| OutputError::DataFusion {
        operation: "apply output geometry schema",
        source,
      })?;
    let parquet_options = writer_options
      .into_table_options(input.schema().as_ref())
      .map_err(|source| OutputError::DataFusion {
        operation: "build Parquet writer options",
        source,
      })?;
    let sink = TrackingSink::create(
      write_path,
      input.schema(),
      Vec::new(),
      parquet_options,
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
  ) -> Result<Arc<dyn ExecutionPlan>, OutputError> {
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
    ProjectionExec::try_new(expressions, input)
      .map(|plan| Arc::new(plan) as Arc<dyn ExecutionPlan>)
      .map_err(|source| OutputError::DataFusion {
        operation: "project output columns",
        source,
      })
  }

  /// Write one DataFrame through partition-local Parquet sinks.
  pub(crate) async fn write_partitioned<RewritePlan>(
    self,
    dataframe: DataFrame,
    write_path: String,
    partition_by: Vec<String>,
    writer_options: WriterOptions,
    rewrite_plan: RewritePlan,
  ) -> Result<u64, OutputError>
  where
    RewritePlan: FnOnce(Arc<dyn ExecutionPlan>) -> DataFusionResult<Arc<dyn ExecutionPlan>>,
  {
    let (state, logical_plan) = dataframe.into_parts();
    let context = Arc::new(TaskContext::from(&state));
    let input = state
      .create_physical_plan(&logical_plan)
      .await
      .map_err(|source| OutputError::DataFusion {
        operation: "create partitioned physical plan",
        source,
      })?;
    let input = rewrite_plan(input).map_err(|source| OutputError::DataFusion {
      operation: "rewrite partitioned physical plan",
      source,
    })?;
    let input = writer_options
      .apply_geometry_schema(input)
      .map_err(|source| OutputError::DataFusion {
        operation: "apply partitioned output geometry schema",
        source,
      })?;
    let parquet_options = writer_options
      .into_table_options(input.schema().as_ref())
      .map_err(|source| OutputError::DataFusion {
        operation: "build partitioned Parquet writer options",
        source,
      })?;
    let sink = TrackingSink::create(
      write_path,
      input.schema(),
      partition_by,
      parquet_options,
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
  ) -> Result<u64, OutputError> {
    Diagnostics::with(label).print_physical_plan(plan.as_ref());
    let batches = match collect(plan, Arc::clone(&context)).await {
      Ok(batches) => batches,
      Err(error) => {
        if let Err(cleanup_error) = sink.cleanup_written_files(&context).await {
          return Err(OutputError::Configuration(format!(
            "parquet write failed: {error}; cleanup failed: {cleanup_error}"
          )));
        }
        return Err(OutputError::DataFusion {
          operation: "execute Parquet sink",
          source: error,
        });
      }
    };
    let batch = batches
      .first()
      .ok_or_else(|| OutputError::Configuration("write returned no row count".to_string()))?;
    let values = batch
      .column(0)
      .as_any()
      .downcast_ref::<UInt64Array>()
      .ok_or_else(|| {
        OutputError::Configuration("write result count column was not UInt64".to_string())
      })?;
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
  use arrow_schema::{DataType, Field, Schema};

  #[test]
  fn compression_parser_rejects_invalid_codec() {
    let error = WriterOptions::parse_compression("bogus").unwrap_err();
    assert!(error.to_string().contains("compression"));
  }

  #[test]
  fn configures_byte_stream_split_by_physical_column_type() {
    let schema = Schema::new(vec![
      Field::new("name", DataType::Utf8, true),
      Field::new("geokey", DataType::UInt64, false),
      Field::new("coordinate", DataType::Float64, false),
      Field::new("geometry", DataType::Binary, true),
      Field::new(
        "properties",
        DataType::Struct(vec![Field::new("category", DataType::LargeUtf8, true)].into()),
        true,
      ),
    ]);
    let options = WriterOptions::new("snappy", &[])
      .unwrap()
      .with_byte_stream_split_columns(["coordinate".to_string()])
      .into_table_options(&schema)
      .unwrap();

    assert_eq!(options.global.dictionary_enabled, Some(false));
    assert_eq!(
      options.column_specific_options["name"].dictionary_enabled,
      Some(true)
    );
    assert_eq!(
      options.column_specific_options["properties.category"].dictionary_enabled,
      Some(true)
    );
    assert!(!options.column_specific_options.contains_key("geokey"));
    assert_eq!(
      options.column_specific_options["coordinate"]
        .encoding
        .as_deref(),
      Some("byte_stream_split")
    );
    assert_eq!(
      options.column_specific_options["coordinate"].dictionary_enabled,
      Some(false)
    );
    assert!(!options.column_specific_options.contains_key("geometry"));
  }
}

//! Owns tracked Parquet sink construction and failed-write cleanup.

use std::fmt;
use std::sync::Arc;

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
use datafusion::object_store::ObjectStoreExt;
use datafusion::physical_plan::{
  DisplayAs, DisplayFormatType, SendableRecordBatchStream, stream::RecordBatchStreamAdapter,
};
use futures_util::StreamExt;

use super::{OutputError, reporter::WriteReporter};

/// Wraps DataFusion's Parquet sink with shared row tracking and failed-write cleanup.
pub(super) struct TrackingSink {
  config: FileSinkConfig,
  inner: ParquetSink,
  reporter: Arc<WriteReporter>,
}

impl TrackingSink {
  /// Create a tracked Parquet sink for one output path and partitioning configuration.
  pub(super) fn create(
    write_path: String,
    output_schema: SchemaRef,
    partition_by: Vec<String>,
    writer_options: TableParquetOptions,
    reporter: Arc<WriteReporter>,
  ) -> Result<Arc<Self>, OutputError> {
    let parsed_url =
      ListingTableUrl::parse(&write_path).map_err(|source| OutputError::DataFusion {
        operation: "parse output listing URL",
        source,
      })?;
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
      file_output_mode: Default::default(),
    };
    Ok(Arc::new(Self::new(config, writer_options, reporter)))
  }

  fn new(
    config: FileSinkConfig,
    parquet_options: TableParquetOptions,
    reporter: Arc<WriteReporter>,
  ) -> Self {
    Self {
      config: config.clone(),
      inner: ParquetSink::new(config, parquet_options),
      reporter,
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

impl fmt::Debug for TrackingSink {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter
      .debug_struct("TrackingSink")
      .finish_non_exhaustive()
  }
}

impl DisplayAs for TrackingSink {
  fn fmt_as(
    &self,
    format_type: DisplayFormatType,
    formatter: &mut fmt::Formatter<'_>,
  ) -> fmt::Result {
    self.inner.fmt_as(format_type, formatter)
  }
}

#[async_trait]
impl DataSink for TrackingSink {
  fn schema(&self) -> &SchemaRef {
    self.inner.schema()
  }

  async fn write_all(
    &self,
    data: SendableRecordBatchStream,
    context: &Arc<TaskContext>,
  ) -> DataFusionResult<u64> {
    let schema = Arc::clone(self.inner.schema());
    let reporter = Arc::clone(&self.reporter);
    let tracked_stream = data.map(move |batch| {
      if let Ok(batch) = &batch {
        reporter.record_batch(batch.num_rows());
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

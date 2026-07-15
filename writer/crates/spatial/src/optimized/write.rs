//! Applies optimized Parquet writer policy to concrete output paths.

use anyhow::{Context, Result};
use datafusion::dataframe::DataFrame;

use crate::optimized::ResolvedOptimization;
use crate::optimized::clustering::{cluster_key_column, cluster_partition_column};
use crate::output::{OutputLayout, ParquetWriterOptions, TrackingParquetWriter};
use crate::pipeline::SharedWriteReporter;

use super::partitioned_sort::PartitionedSortConfig;

/// Writes one range-partitioned optimized dataset through the custom sink plan.
pub(super) struct PartitionedOutputWriter<'a> {
  output_layout: &'a OutputLayout,
  compression: Option<&'a str>,
  optimization: &'a ResolvedOptimization,
  total_input_rows: u64,
  write_reporter: Option<SharedWriteReporter>,
}

/// Write globally sorted optimized rows through the shared tracking sink.
pub(super) async fn write_optimized_single_file(
  dataframe: DataFrame,
  output_layout: &OutputLayout,
  compression: Option<&str>,
  metadata: Vec<parquet::file::metadata::KeyValue>,
  hidden_sort_column: Option<&str>,
  total_input_rows: u64,
  write_reporter: Option<SharedWriteReporter>,
) -> Result<u64> {
  let writer_options =
    ParquetWriterOptions::new(compression.unwrap_or("snappy"), &metadata)?.into_datafusion();
  let output_path = output_layout
    .paths()?
    .into_iter()
    .next()
    .context("missing output path")?
    .to_string_lossy()
    .into_owned();
  TrackingParquetWriter::new(total_input_rows, write_reporter)
    .write_single(
      dataframe,
      output_path,
      writer_options,
      hidden_sort_column.into_iter().collect(),
    )
    .await
}

impl<'a> PartitionedOutputWriter<'a> {
  /// Construct partitioned output writing for one resolved optimization.
  pub(super) fn new(
    output_layout: &'a OutputLayout,
    compression: Option<&'a str>,
    optimization: &'a ResolvedOptimization,
    total_input_rows: u64,
    write_reporter: Option<SharedWriteReporter>,
  ) -> Self {
    Self {
      output_layout,
      compression,
      optimization,
      total_input_rows,
      write_reporter,
    }
  }

  /// Write range-partitioned optimized rows through the custom physical sink plan.
  pub(super) async fn write(
    self,
    dataframe: DataFrame,
    metadata: Vec<parquet::file::metadata::KeyValue>,
  ) -> Result<u64> {
    let writer_options =
      ParquetWriterOptions::new(self.compression.unwrap_or("snappy"), &metadata)?.into_datafusion();
    let partition_column = cluster_partition_column(self.optimization.geometry().clustering_family);
    let partitioned_sort = PartitionedSortConfig::new(
      partition_column,
      cluster_key_column(self.optimization.geometry().clustering_family),
      self.output_layout.part_count(),
      true,
    );
    TrackingParquetWriter::new(self.total_input_rows, self.write_reporter)
      .write_partitioned(
        dataframe,
        self.output_layout.path().to_string_lossy().into_owned(),
        vec![partition_column.to_string()],
        writer_options,
        |input| partitioned_sort.insert_into(input),
      )
      .await
  }
}

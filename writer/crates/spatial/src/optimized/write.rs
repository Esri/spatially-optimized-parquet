//! Applies optimized Parquet writer policy to concrete output paths.

use anyhow::{Context, Result};
use datafusion::dataframe::DataFrame;
use engine::{OutputLayout, ParquetWriterOptions, write_single_file};

use crate::optimized::clustering::{cluster_key_column, cluster_partition_column};
use crate::optimized::{ClusteringFamily, ResolvedOptimization};
use crate::progress::{WriteStagePhase, finish_spinner, row_bar, write_stage_message};

use super::partitioned_sink::PartitionedParquetWriter;
use super::partitioned_sort::PartitionedSortConfig;

/// Writes one range-partitioned optimized dataset through the custom sink plan.
pub(super) struct PartitionedOutputWriter<'a> {
  output_layout: &'a OutputLayout,
  compression: Option<&'a str>,
  optimization: &'a ResolvedOptimization,
  progress: bool,
  total_input_rows: u64,
  explain: bool,
}

/// Write globally sorted optimized rows through DataFusion's standard single-file API.
pub(super) async fn write_optimized_single_file(
  dataframe: DataFrame,
  output_layout: &OutputLayout,
  compression: Option<&str>,
  metadata: Vec<parquet::file::metadata::KeyValue>,
  progress: bool,
  total_input_rows: u64,
) -> Result<u64> {
  let writer_options = ParquetWriterOptions::new(compression.unwrap_or("snappy"), &metadata)?;
  let write_bar = row_bar(
    progress,
    write_stage_message(WriteStagePhase::Reading, false),
    total_input_rows,
  );
  let output_path = output_layout
    .paths()?
    .into_iter()
    .next()
    .context("missing output path")?
    .to_string_lossy()
    .into_owned();
  let rows_written = write_single_file(dataframe, &output_path, writer_options).await?;
  finish_spinner(
    &write_bar,
    format!("Completed write pipeline ({rows_written} rows)"),
  );
  Ok(rows_written)
}

impl<'a> PartitionedOutputWriter<'a> {
  /// Construct partitioned output writing for one resolved optimization.
  pub(super) fn new(
    output_layout: &'a OutputLayout,
    compression: Option<&'a str>,
    optimization: &'a ResolvedOptimization,
    progress: bool,
    total_input_rows: u64,
    explain: bool,
  ) -> Self {
    Self {
      output_layout,
      compression,
      optimization,
      progress,
      total_input_rows,
      explain,
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
    let write_bar = row_bar(
      self.progress,
      write_stage_message(WriteStagePhase::Reading, true),
      self.total_input_rows,
    );
    let partition_column = cluster_partition_column(self.optimization.geometry().clustering_family);
    let drop_cluster_key_after_sort = matches!(
      self.optimization.geometry().clustering_family,
      ClusteringFamily::NonPoint
    );
    let rows_written = PartitionedParquetWriter::new(
      self.output_layout.path().to_string_lossy().into_owned(),
      vec![partition_column.to_string()],
      PartitionedSortConfig::new(
        partition_column,
        cluster_key_column(self.optimization.geometry().clustering_family),
        self.output_layout.part_count(),
        drop_cluster_key_after_sort,
      ),
      writer_options,
      &write_bar,
      self.total_input_rows,
      self.explain,
    )
    .write(dataframe)
    .await?;
    finish_spinner(
      &write_bar,
      format!("Completed write pipeline ({rows_written} rows)"),
    );
    Ok(rows_written)
  }
}

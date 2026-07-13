//! Applies optimized Parquet writer policy to concrete output paths.

use anyhow::{Context, Result};
use engine::output_layout::{OutputLayout, resolved_output_paths};
use engine::parquet_write::{
  create_datafusion_parquet_options, parse_compression, write_single_file,
};

use crate::optimized::clustering::{cluster_key_column, cluster_partition_column};
use crate::optimized::{ClusteringFamily, ResolvedOptimization};
use crate::progress::{WriteStagePhase, finish_spinner, row_bar, write_stage_message};

use super::partitioned_sink::{PartitionedWriteRequest, write_partitioned_parquet};
use super::partitioned_sort::PartitionedSortConfig;

/// Writes one range-partitioned optimized dataset through the custom sink plan.
pub(crate) struct PartitionedOutputWriter<'a> {
  output_layout: &'a OutputLayout,
  compression: Option<&'a str>,
  optimization: &'a ResolvedOptimization,
  progress: bool,
  total_input_rows: u64,
  explain: bool,
}

/// Write globally sorted optimized rows through DataFusion's standard single-file API.
pub(crate) async fn write_optimized_single_file(
  dataframe: engine::DataFrame,
  output_layout: &OutputLayout,
  compression: Option<&str>,
  metadata: Vec<parquet::file::metadata::KeyValue>,
  progress: bool,
  total_input_rows: u64,
) -> Result<u64> {
  let writer_options = create_datafusion_parquet_options(
    parse_compression(compression.unwrap_or("snappy"))?,
    &metadata,
  );
  let write_bar = row_bar(
    progress,
    write_stage_message(WriteStagePhase::Reading, false),
    total_input_rows,
  );
  let output_path = resolved_output_paths(output_layout)?
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
  pub(crate) fn new(
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
  pub(crate) async fn write(
    self,
    dataframe: engine::DataFrame,
    metadata: Vec<parquet::file::metadata::KeyValue>,
  ) -> Result<u64> {
    let writer_options = create_datafusion_parquet_options(
      parse_compression(self.compression.unwrap_or("snappy"))?,
      &metadata,
    );
    let write_bar = row_bar(
      self.progress,
      write_stage_message(WriteStagePhase::Reading, true),
      self.total_input_rows,
    );
    let partition_column = cluster_partition_column(self.optimization.geometry.clustering_family);
    let drop_cluster_key_after_sort = matches!(
      self.optimization.geometry.clustering_family,
      ClusteringFamily::NonPoint
    );
    let rows_written = write_partitioned_parquet(
      dataframe,
      PartitionedWriteRequest {
        write_path: self.output_layout.path.to_string_lossy().into_owned(),
        partition_by: vec![partition_column.to_string()],
        partitioned_sort: PartitionedSortConfig::new(
          partition_column,
          cluster_key_column(self.optimization.geometry.clustering_family),
          self.output_layout.parts,
          drop_cluster_key_after_sort,
        ),
        writer_options,
        progress_bar: &write_bar,
        total_input_rows: self.total_input_rows,
        explain: self.explain,
      },
    )
    .await?;
    finish_spinner(
      &write_bar,
      format!("Completed write pipeline ({rows_written} rows)"),
    );
    Ok(rows_written)
  }
}

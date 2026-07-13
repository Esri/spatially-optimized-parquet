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

/// Write range-partitioned optimized rows through the custom physical sink plan.
pub(crate) async fn write_optimized_partitioned(
  dataframe: engine::DataFrame,
  output_layout: &OutputLayout,
  compression: Option<&str>,
  optimization: &ResolvedOptimization,
  metadata: Vec<parquet::file::metadata::KeyValue>,
  progress: bool,
  total_input_rows: u64,
  explain: bool,
) -> Result<u64> {
  let writer_options = create_datafusion_parquet_options(
    parse_compression(compression.unwrap_or("snappy"))?,
    &metadata,
  );
  let write_bar = row_bar(
    progress,
    write_stage_message(WriteStagePhase::Reading, true),
    total_input_rows,
  );
  let partition_column = cluster_partition_column(optimization.geometry.clustering_family);
  let drop_cluster_key_after_sort = matches!(
    optimization.geometry.clustering_family,
    ClusteringFamily::NonPoint
  );
  let rows_written = write_partitioned_parquet(
    dataframe,
    PartitionedWriteRequest {
      write_path: output_layout.path.to_string_lossy().into_owned(),
      partition_by: vec![partition_column.to_string()],
      partitioned_sort: PartitionedSortConfig {
        partition_column: partition_column.to_string(),
        cluster_key_column: cluster_key_column(optimization.geometry.clustering_family).to_string(),
        bucket_count: output_layout.parts,
        drop_cluster_key_after_sort,
      },
      writer_options,
      progress_bar: &write_bar,
      total_input_rows,
      explain,
    },
  )
  .await?;
  finish_spinner(
    &write_bar,
    format!("Completed write pipeline ({rows_written} rows)"),
  );
  Ok(rows_written)
}

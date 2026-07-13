//! Orchestrates optimized output paths and Parquet writer policy.

use anyhow::{Context, Result};
use engine::output_layout::resolved_output_paths;
use engine::parquet_write::{
  create_datafusion_parquet_options, parse_compression, write_single_file,
};

use crate::optimized::clustering::{cluster_key_column, cluster_partition_column};
use crate::optimized::{ClusteringFamily, ResolvedOptimization};
use crate::output::stage::OutputStageContext;
use crate::progress::{WriteStagePhase, finish_spinner, row_bar, write_stage_message};

use super::partitioned_sink::{PartitionedWriteRequest, write_partitioned_parquet};
use super::partitioned_sort::PartitionedSortConfig;

/// Configure the final Parquet sink and execute the optimized projection.
pub(crate) async fn write_optimized_output(
  request: &OutputStageContext<'_>,
  dataframe: engine::DataFrame,
  optimization: &ResolvedOptimization,
  metadata: Vec<parquet::file::metadata::KeyValue>,
) -> Result<u64> {
  let compression = parse_compression(request.compression.unwrap_or("snappy"))?;
  let writer_options = create_datafusion_parquet_options(compression, &metadata);
  let partitioned_output = request.output_layout.parts > 1;
  let write_bar = row_bar(
    request.progress,
    write_stage_message(WriteStagePhase::Reading, partitioned_output),
    request.total_input_rows,
  );
  let rows_written = if partitioned_output {
    let partition_column = cluster_partition_column(optimization.geometry.clustering_family);
    let drop_cluster_key_after_sort = matches!(
      optimization.geometry.clustering_family,
      ClusteringFamily::NonPoint
    );
    write_partitioned_parquet(
      dataframe,
      PartitionedWriteRequest {
        write_path: request.output_layout.path.to_string_lossy().into_owned(),
        partition_by: vec![partition_column.to_string()],
        partitioned_sort: PartitionedSortConfig {
          partition_column: partition_column.to_string(),
          cluster_key_column: cluster_key_column(optimization.geometry.clustering_family)
            .to_string(),
          bucket_count: request.output_layout.parts,
          drop_cluster_key_after_sort,
        },
        writer_options,
        progress_bar: &write_bar,
        total_input_rows: request.total_input_rows,
        explain: request.explain,
      },
    )
    .await?
  } else {
    let output_path = resolved_output_paths(request.output_layout)?
      .into_iter()
      .next()
      .context("missing output path")?
      .to_string_lossy()
      .into_owned();
    write_single_file(dataframe, &output_path, writer_options).await?
  };
  finish_spinner(
    &write_bar,
    format!("Completed write pipeline ({rows_written} rows)"),
  );
  Ok(rows_written)
}

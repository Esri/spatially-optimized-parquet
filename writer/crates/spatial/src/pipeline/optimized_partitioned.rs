//! Executes range-partitioned optimized GeoParquet.

use anyhow::Result;

use crate::optimized::clustering::{
  cluster_key_column, cluster_partition_column, validate_cluster_partition_column,
};
use crate::optimized::range_boundaries::compute_cluster_range_boundaries;
use crate::optimized::write::PartitionedOutputWriter;
use crate::progress::{finish_row_bar, row_bar};

use super::optimized::resolve_optimization;
use super::{OptimizedPartitionedPipeline, SpatialPipelineResult};

impl OptimizedPartitionedPipeline {
  pub(super) async fn execute(self) -> Result<SpatialPipelineResult> {
    let state = self.state;
    let optimization = resolve_optimization(&state).await?;
    validate_cluster_partition_column(
      state.source_schema.as_ref(),
      Some(cluster_partition_column(
        optimization.geometry.clustering_family,
      )),
    )?;
    let range_bar = row_bar(
      state.progress,
      "Computing partition ranges",
      state.total_input_rows,
    );
    let range_source = optimization.partitioned_range_source(state.input_dataframe.clone())?;
    let boundaries = compute_cluster_range_boundaries(
      range_source,
      cluster_key_column(optimization.geometry.clustering_family),
      state.output_layout.parts,
      &range_bar,
      state.total_input_rows,
      state.explain,
    )
    .await?;
    finish_row_bar(
      &range_bar,
      state.total_input_rows,
      "Computed partition ranges".to_string(),
    );
    let dataframe = optimization.partitioned_projection(
      state.input_dataframe.clone(),
      state.source_schema.as_ref(),
      &boundaries,
      state.covering,
    )?;
    let metadata = optimization.parquet_metadata(state.covering)?;
    let rows_written = PartitionedOutputWriter::new(
      &state.output_layout,
      state.compression.as_deref(),
      &optimization,
      state.progress,
      state.total_input_rows,
      state.explain,
    )
    .write(dataframe, metadata)
    .await?;
    Ok(state.finish(rows_written))
  }
}

//! Executes globally sorted optimized single-file GeoParquet.

use anyhow::Result;

use crate::optimized::write::write_optimized_single_file;

use super::optimized::resolve_optimization;
use super::{OptimizedSingleFilePipeline, SpatialPipelineResult};

impl OptimizedSingleFilePipeline {
  pub(super) async fn execute(self) -> Result<SpatialPipelineResult> {
    let state = self.state;
    let optimization = resolve_optimization(&state).await?;
    let dataframe = optimization.single_file_projection(
      state.input_dataframe.clone(),
      state.source_schema.as_ref(),
      state.covering,
    )?;
    let metadata = optimization.parquet_metadata(state.covering)?;
    let rows_written = write_optimized_single_file(
      dataframe,
      &state.output_layout,
      state.compression.as_deref(),
      metadata,
      state.progress,
      state.total_input_rows,
    )
    .await?;
    Ok(state.finish(rows_written))
  }
}

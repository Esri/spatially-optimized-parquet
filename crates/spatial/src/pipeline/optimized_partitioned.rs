//! Executes range-partitioned optimized GeoParquet.

use anyhow::Result;

use super::optimized::OptimizedOutput;
use super::{OptimizedPartitionedPipeline, SpatialPipelineResult};

impl OptimizedPartitionedPipeline {
  pub(super) async fn execute(self) -> Result<SpatialPipelineResult> {
    let state = self.state;
    let output = OptimizedOutput::new(
      state.input.as_ref(),
      state.input_dataframe.clone(),
      &state.output_layout,
      state.source_schema.as_ref(),
      state.total_input_rows,
      state.row_range,
      state.write_reporter.clone(),
      state.warning_store.clone(),
    )
    .resolve(
      state.geometry_column.as_deref(),
      state.input_wkid,
      state.output_wkid,
      state.covering,
      state.strip_z,
      state.strip_m,
      state.compression.as_deref(),
    )
    .await?;
    let rows_written = output.write_partitioned().await?;
    Ok(state.finish(rows_written))
  }
}

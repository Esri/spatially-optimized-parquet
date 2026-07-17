//! Writes range-partitioned optimized GeoParquet.

use anyhow::Result;

use super::{OptimizedPartitionedPipeline, SpatialPipelineResult};
use crate::optimized::OptimizedGeoParquetWriter;

impl OptimizedPartitionedPipeline {
  pub(super) async fn execute(self) -> Result<SpatialPipelineResult> {
    let state = self.state;
    let writer = OptimizedGeoParquetWriter::new(
      state.input.as_ref(),
      state.input_dataframe.clone(),
      &state.output_layout,
      state.source_schema.as_ref(),
      state.total_input_rows,
      state.row_range,
      state.write_reporter.clone(),
      state.warning_store.clone(),
    )
    .resolve(&state.output_options)
    .await?;
    let rows_written = writer.write_partitioned().await?;
    Ok(state.finish(rows_written))
  }
}

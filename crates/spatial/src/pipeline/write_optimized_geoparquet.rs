//! Writes globally sorted optimized single-file GeoParquet.

use anyhow::Result;

use super::{OptimizedSingleFilePipeline, SpatialPipelineResult};
use crate::optimized::{resolve_optimized_geoparquet, single};

impl OptimizedSingleFilePipeline {
  pub(super) async fn execute(self) -> Result<SpatialPipelineResult> {
    let state = self.state;
    let (dataframe, optimization) = resolve_optimized_geoparquet(
      state.input.as_ref(),
      state.input_dataframe.clone(),
      state.source_schema.as_ref(),
      state.row_range,
      &state.output_options,
    )
    .await?;
    let rows_written = single::write(
      dataframe,
      &state.output_path,
      state.source_schema.as_ref(),
      &optimization,
      state.output_options.covering,
      state.output_options.compression.as_deref(),
      state.total_input_rows,
      state.write_reporter.clone(),
      state.warning_store.clone(),
    )
    .await?;
    Ok(state.finish(rows_written))
  }
}

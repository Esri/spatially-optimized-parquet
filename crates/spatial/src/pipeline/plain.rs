//! Executes normalized single-file GeoParquet.

use anyhow::Result;

use crate::geoparquet::PlainOutput;

use super::{PlainPipeline, SpatialPipelineResult};

impl PlainPipeline {
  pub(super) async fn execute(self) -> Result<SpatialPipelineResult> {
    let state = self.state;
    let rows_written = PlainOutput::new(
      state.input.as_ref(),
      state.input_dataframe.clone(),
      &state.output_layout,
      state.source_schema.as_ref(),
      state.geometry_column.as_deref(),
      state.input_wkid,
      state.row_range,
      state.total_input_rows,
      state.write_reporter.clone(),
    )
    .write(
      state.output_wkid,
      state.covering,
      state.strip_z,
      state.strip_m,
      state.compression.as_deref(),
    )
    .await?;
    Ok(state.finish(rows_written))
  }
}

//! Executes normalized single-file GeoParquet.

use anyhow::Result;

use crate::diagnostics::explain_stage_note;
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
    )
    .write(
      state.output_wkid,
      state.covering,
      state.compression.as_deref(),
    )
    .await?;
    explain_stage_note(
      state.explain,
      "Plain GeoParquet",
      &format!("wrote {rows_written} selected rows without optimized clustering"),
    );
    Ok(state.finish(rows_written))
  }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

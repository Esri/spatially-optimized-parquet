//! Executes normalized single-file GeoParquet.

use anyhow::Result;

use crate::geoparquet::GeoParquetWriter;

use super::{PlainPipeline, SpatialPipelineResult};

impl PlainPipeline {
  pub(super) async fn execute(self) -> Result<SpatialPipelineResult> {
    let state = self.state;
    let options = &state.output_options;
    let rows_written = GeoParquetWriter::new(
      state.input.as_ref(),
      state.input_dataframe.clone(),
      &state.output_layout,
      state.source_schema.as_ref(),
      options.geometry_column.as_deref(),
      options.input_wkid,
      state.row_range,
      state.total_input_rows,
      state.write_reporter.clone(),
    )
    .write(
      options.output_wkid,
      options.covering,
      options.strip_z,
      options.strip_m,
      options.compression.as_deref(),
    )
    .await?;
    Ok(state.finish(rows_written))
  }
}

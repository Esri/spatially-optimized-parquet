//! Persists globally ordered optimized output into one Parquet file.

use anyhow::{Context, Result};
use datafusion::dataframe::DataFrame;

use crate::optimized::ResolvedOptimization;
use crate::output::{OutputPath, Writer, WriterOptions};
use crate::pipeline::{PipelineWarnings, SharedWriteReporter};

use super::dataframe;

/// Write one globally ordered optimized GeoParquet file.
pub(crate) async fn write(
  input_dataframe: DataFrame,
  output_path: &OutputPath,
  source_schema: &arrow_schema::Schema,
  optimization: &ResolvedOptimization,
  covering: bool,
  compression: Option<&str>,
  total_input_rows: u64,
  write_reporter: Option<SharedWriteReporter>,
  warnings: PipelineWarnings,
) -> Result<u64> {
  let dataframe = dataframe::dataframe(
    input_dataframe,
    source_schema,
    optimization,
    covering,
    warnings,
  )?;
  let metadata = optimization.parquet_metadata(covering)?;
  let writer_options = WriterOptions::new(compression.unwrap_or("snappy"), &metadata)?
    .with_delta_binary_packed_columns(optimization.delta_binary_packed_column_paths());
  let output_path = output_path
    .paths()?
    .into_iter()
    .next()
    .context("missing output path")?
    .to_string_lossy()
    .into_owned();
  Writer::new(total_input_rows, write_reporter)
    .write_single(
      dataframe,
      output_path,
      writer_options,
      vec![
        optimization
          .geometry()
          .clustering_family
          .cluster_key_column(),
      ],
    )
    .await
}

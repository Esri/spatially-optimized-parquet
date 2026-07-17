//! Persists globally ordered optimized output into one Parquet file.

use crate::geoparquet::GeoParquetWriteContext;
use crate::optimized::OptimizedLayout;
use crate::output::{OutputPath, Writer, WriterOptions};
use crate::pipeline::{PipelineWarnings, SharedWriteReporter};
use anyhow::{Context, Result};

use super::dataframe;

/// Write one globally ordered optimized GeoParquet file.
pub(crate) async fn write(
  output_path: &OutputPath,
  source_schema: &arrow_schema::Schema,
  context: &GeoParquetWriteContext,
  layout: &OptimizedLayout,
  covering: bool,
  compression: Option<&str>,
  total_input_rows: u64,
  write_reporter: Option<SharedWriteReporter>,
  warnings: PipelineWarnings,
) -> Result<u64> {
  let dataframe = dataframe::dataframe(source_schema, context, layout, covering, warnings)?;
  let metadata = layout.parquet_metadata(context, covering)?;
  let writer_options = WriterOptions::new(compression.unwrap_or("snappy"), &metadata)?
    .with_delta_binary_packed_columns(layout.delta_binary_packed_column_paths());
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
      vec![layout.geometry().clustering_family.cluster_key_column()],
    )
    .await
}

//! Writes standard single-file GeoParquet through DataFusion `COPY TO`.
//!
//! The input remains a lazy DataFrame. DataFusion executes source scans, Arrow operators, and
//! Parquet encoding. This module only supplies repository writer options and extracts the row
//! count returned by DataFusion's write command.

use anyhow::{Context, Result};
use arrow_array::{Array, RecordBatch, UInt64Array};
use datafusion::common::config::TableParquetOptions;
use datafusion::dataframe::DataFrameWriteOptions;
use engine::DataFrame;

/// Execute a lazy DataFrame as one Parquet file and return the written row count.
pub(crate) async fn write_dataframe(
  dataframe: DataFrame,
  output_path: &str,
  options: TableParquetOptions,
) -> Result<u64> {
  let batches = dataframe
    .write_parquet(
      output_path,
      DataFrameWriteOptions::new().with_single_file_output(true),
      Some(options),
    )
    .await?;
  extract_written_row_count(&batches)
}

fn extract_written_row_count(batches: &[RecordBatch]) -> Result<u64> {
  let batch = batches.first().context("write returned no row count")?;
  let values = batch
    .column(0)
    .as_any()
    .downcast_ref::<UInt64Array>()
    .context("write result count column was not UInt64")?;
  if values.is_empty() {
    return Ok(0);
  }
  Ok(values.value(0))
}

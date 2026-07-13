//! Centralizes Parquet writer policy and single-file execution.
//!
//! The module parses supported compression names, disables dictionary encoding, applies
//! consistent row-group and write-batch sizes, and propagates key-value metadata through
//! [`create_datafusion_parquet_options`].
//!
//! Row-group and batch sizes can be tuned through `OPT_PARQUET_ROW_GROUP_SIZE` and
//! `OPT_PARQUET_WRITE_BATCH_SIZE`. Larger values can improve compression and throughput at
//! the cost of memory, while smaller values reduce buffering and may increase file overhead.

use anyhow::{Context, Result};
use arrow_array::{Array, RecordBatch, UInt64Array};
use datafusion::common::config::TableParquetOptions;
use datafusion::dataframe::DataFrame;
use datafusion::dataframe::DataFrameWriteOptions;
use parquet::basic::{BrotliLevel, Compression, GzipLevel, ZstdLevel};
use parquet::file::metadata::KeyValue;

const DEFAULT_MAX_ROW_GROUP_SIZE: usize = 128 * 1024;
const DEFAULT_WRITE_BATCH_SIZE: usize = 8 * 1024;
const ROW_GROUP_SIZE_ENV: &str = "OPT_PARQUET_ROW_GROUP_SIZE";
const WRITE_BATCH_SIZE_ENV: &str = "OPT_PARQUET_WRITE_BATCH_SIZE";

/// Parse a user-facing compression name into a Parquet codec.
pub fn parse_compression(compression: &str) -> Result<Compression> {
  let codec = match compression.to_ascii_lowercase().as_str() {
    "snappy" => Compression::SNAPPY,
    "gzip" => Compression::GZIP(GzipLevel::default()),
    "brotli" => Compression::BROTLI(BrotliLevel::default()),
    "lz4" => Compression::LZ4,
    "lz4_raw" => Compression::LZ4_RAW,
    "zstd" => Compression::ZSTD(ZstdLevel::default()),
    "uncompressed" => Compression::UNCOMPRESSED,
    other => {
      return Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!("invalid compression: {other}"),
      ))
      .context("parse compression");
    }
  };
  Ok(codec)
}

/// Build repository-wide DataFusion Parquet options.
pub fn create_datafusion_parquet_options(
  compression: Compression,
  kv_metadata: &[KeyValue],
) -> TableParquetOptions {
  let mut options = TableParquetOptions::new();
  options.global.compression = Some(compression_to_datafusion_string(compression));
  options.global.dictionary_enabled = Some(false);
  options.global.max_row_group_size = configured_max_row_group_size();
  options.global.write_batch_size = configured_write_batch_size();
  options.key_value_metadata = kv_metadata
    .iter()
    .map(|kv| (kv.key.clone(), kv.value.clone()))
    .collect();
  options
}

/// Write one DataFrame to a single Parquet file and return its row count.
pub async fn write_single_file(
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
  written_row_count(&batches)
}

/// Decode the row count returned by a DataFusion write operation.
pub fn written_row_count(batches: &[RecordBatch]) -> Result<u64> {
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

fn compression_to_datafusion_string(compression: Compression) -> String {
  match compression {
    Compression::UNCOMPRESSED => "uncompressed".to_string(),
    Compression::SNAPPY => "snappy".to_string(),
    Compression::GZIP(level) => format!("gzip({})", level.compression_level()),
    Compression::LZO => "lzo".to_string(),
    Compression::BROTLI(level) => format!("brotli({})", level.compression_level()),
    Compression::LZ4 => "lz4".to_string(),
    Compression::ZSTD(level) => format!("zstd({})", level.compression_level()),
    Compression::LZ4_RAW => "lz4_raw".to_string(),
  }
}

fn configured_max_row_group_size() -> usize {
  env_usize(ROW_GROUP_SIZE_ENV).unwrap_or(DEFAULT_MAX_ROW_GROUP_SIZE)
}

fn configured_write_batch_size() -> usize {
  env_usize(WRITE_BATCH_SIZE_ENV).unwrap_or(DEFAULT_WRITE_BATCH_SIZE)
}

fn env_usize(name: &str) -> Option<usize> {
  std::env::var(name)
    .ok()
    .and_then(|value| value.parse::<usize>().ok())
    .filter(|value| *value > 0)
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use arrow_array::{RecordBatch, StringArray, UInt64Array};
  use arrow_schema::{DataType, Field, Schema};

  use super::written_row_count;

  #[test]
  fn written_row_count_decodes_valid_and_empty_arrays() {
    let schema = Arc::new(Schema::new(vec![Field::new(
      "count",
      DataType::UInt64,
      false,
    )]));
    let valid = RecordBatch::try_new(
      Arc::clone(&schema),
      vec![Arc::new(UInt64Array::from(vec![42]))],
    )
    .unwrap();
    let empty =
      RecordBatch::try_new(schema, vec![Arc::new(UInt64Array::from(Vec::<u64>::new()))]).unwrap();

    assert_eq!(written_row_count(&[valid]).unwrap(), 42);
    assert_eq!(written_row_count(&[empty]).unwrap(), 0);
  }

  #[test]
  fn written_row_count_rejects_missing_and_malformed_results() {
    let schema = Arc::new(Schema::new(vec![Field::new(
      "count",
      DataType::Utf8,
      false,
    )]));
    let malformed = RecordBatch::try_new(
      schema,
      vec![Arc::new(StringArray::from(vec!["not-a-count"]))],
    )
    .unwrap();

    assert!(
      written_row_count(&[])
        .unwrap_err()
        .to_string()
        .contains("no row count")
    );
    assert!(
      written_row_count(&[malformed])
        .unwrap_err()
        .to_string()
        .contains("was not UInt64")
    );
  }
}

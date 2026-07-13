//! Centralizes Parquet writer policy for DataFusion sinks.
//!
//! The module parses supported compression names, disables dictionary encoding, applies
//! consistent row-group and write-batch sizes, and propagates key-value metadata through
//! [`create_datafusion_parquet_options`].
//!
//! Row-group and batch sizes can be tuned through `OPT_PARQUET_ROW_GROUP_SIZE` and
//! `OPT_PARQUET_WRITE_BATCH_SIZE`. Larger values can improve compression and throughput at
//! the cost of memory, while smaller values reduce buffering and may increase file overhead.

use anyhow::{Context, Result};
use datafusion::common::config::TableParquetOptions;
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

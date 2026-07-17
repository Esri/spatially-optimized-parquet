//! Centralizes Parquet compression, metadata, and buffering policy.
//!
//! [`ParquetWriterOptions`] parses supported compression names, enables dictionary encoding,
//! applies consistent row-group and write-batch sizes, and propagates key-value metadata.
//!
//! Row-group and batch sizes can be tuned through `OPT_PARQUET_ROW_GROUP_SIZE` and
//! `OPT_PARQUET_WRITE_BATCH_SIZE`. Larger values can improve compression and throughput at
//! the cost of memory, while smaller values reduce buffering and may increase file overhead.

use anyhow::{Context, Result};
use datafusion::common::config::ParquetColumnOptions;
use datafusion::common::config::TableParquetOptions;
use datafusion::common::parquet_config::DFParquetWriterVersion;
use parquet::basic::{BrotliLevel, Compression, GzipLevel, ZstdLevel};
use parquet::file::metadata::KeyValue;

const DEFAULT_MAX_ROW_GROUP_SIZE: usize = 128 * 1024;
const DEFAULT_WRITE_BATCH_SIZE: usize = 8 * 1024;
const ROW_GROUP_SIZE_ENV: &str = "OPT_PARQUET_ROW_GROUP_SIZE";
const WRITE_BATCH_SIZE_ENV: &str = "OPT_PARQUET_WRITE_BATCH_SIZE";

/// Owns the configured DataFusion options for one Parquet write.
pub(crate) struct ParquetWriterOptions {
  options: TableParquetOptions,
}

impl ParquetWriterOptions {
  /// Build Parquet writer options from a compression name and key-value metadata.
  pub(crate) fn new(compression: &str, kv_metadata: &[KeyValue]) -> Result<Self> {
    let compression = Self::parse_compression(compression)?;
    let mut options = TableParquetOptions::new();
    options.global.compression = Some(Self::datafusion_compression_name(compression));
    options.global.dictionary_enabled = Some(true);
    options.global.writer_version = DFParquetWriterVersion::V2_0;
    options.global.maximum_parallel_row_group_writers = Self::available_parallelism();
    options.global.max_row_group_size = Self::configured_max_row_group_size();
    options.global.write_batch_size = Self::configured_write_batch_size();
    options.key_value_metadata = kv_metadata
      .iter()
      .map(|kv| (kv.key.clone(), kv.value.clone()))
      .collect();
    Ok(Self { options })
  }

  /// Consume the typed options for a custom DataFusion Parquet sink.
  pub(crate) fn into_datafusion(self) -> TableParquetOptions {
    self.options
  }

  /// Apply integer delta packing to selected physical coordinate leaves.
  pub(crate) fn with_delta_binary_packed_columns(
    mut self,
    columns: impl IntoIterator<Item = String>,
  ) -> Self {
    for column in columns {
      self.options.column_specific_options.insert(
        column,
        ParquetColumnOptions {
          encoding: Some("delta_binary_packed".to_string()),
          dictionary_enabled: Some(false),
          ..Default::default()
        },
      );
    }
    self
  }

  fn parse_compression(compression: &str) -> Result<Compression> {
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

  fn datafusion_compression_name(compression: Compression) -> String {
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
    Self::env_usize(ROW_GROUP_SIZE_ENV).unwrap_or(DEFAULT_MAX_ROW_GROUP_SIZE)
  }

  fn configured_write_batch_size() -> usize {
    Self::env_usize(WRITE_BATCH_SIZE_ENV).unwrap_or(DEFAULT_WRITE_BATCH_SIZE)
  }

  fn available_parallelism() -> usize {
    std::thread::available_parallelism()
      .map(usize::from)
      .unwrap_or(1)
  }

  fn env_usize(name: &str) -> Option<usize> {
    std::env::var(name)
      .ok()
      .and_then(|value| value.parse::<usize>().ok())
      .filter(|value| *value > 0)
  }
}

#[cfg(test)]
mod tests {
  use super::ParquetWriterOptions;

  #[test]
  fn compression_parser_rejects_invalid_codec() {
    let error = ParquetWriterOptions::parse_compression("bogus").unwrap_err();
    assert!(error.to_string().contains("compression"));
  }
}

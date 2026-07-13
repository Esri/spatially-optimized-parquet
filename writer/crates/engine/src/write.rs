//! Centralizes Parquet writer policy for both direct Arrow writers and DataFusion sinks.
//!
//! The module parses supported compression names, disables dictionary encoding, applies
//! consistent row-group and write-batch sizes, propagates key-value metadata, and tracks
//! direct-writer row counts. [`create_datafusion_parquet_options`] mirrors the settings used
//! by [`create_output_writer`] so output characteristics do not depend on the execution path.
//!
//! Row-group and batch sizes can be tuned through `OPT_PARQUET_ROW_GROUP_SIZE` and
//! `OPT_PARQUET_WRITE_BATCH_SIZE`. Larger values can improve compression and throughput at
//! the cost of memory, while smaller values reduce buffering and may increase file overhead.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use arrow_array::RecordBatch;
use arrow_schema::SchemaRef;
use datafusion::common::config::TableParquetOptions;
use parquet::arrow::arrow_writer::ArrowWriter;
use parquet::basic::{BrotliLevel, Compression, GzipLevel, ZstdLevel};
use parquet::file::metadata::KeyValue;
use parquet::file::properties::WriterProperties;

const DEFAULT_MAX_ROW_GROUP_SIZE: usize = 128 * 1024;
const DEFAULT_WRITE_BATCH_SIZE: usize = 8 * 1024;
const ROW_GROUP_SIZE_ENV: &str = "OPT_PARQUET_ROW_GROUP_SIZE";
const WRITE_BATCH_SIZE_ENV: &str = "OPT_PARQUET_WRITE_BATCH_SIZE";

/// Owns a direct Arrow Parquet writer and its committed row count.
pub struct OutputWriter {
  writer: Option<ArrowWriter<fs::File>>,
  /// Tracks rows accepted by the underlying Parquet writer.
  pub rows_written: u64,
}

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

/// Create a direct Parquet writer with repository-wide row-group and batch settings.
pub fn create_output_writer(
  path: &Path,
  schema: &SchemaRef,
  compression: Compression,
) -> Result<OutputWriter> {
  let file =
    fs::File::create(path).with_context(|| format!("create output file: {}", path.display()))?;
  let writer_properties = WriterProperties::builder()
    .set_compression(compression)
    .set_dictionary_enabled(false)
    .set_max_row_group_size(configured_max_row_group_size())
    .set_write_batch_size(configured_write_batch_size())
    .build();
  let writer = ArrowWriter::try_new(file, schema.clone(), Some(writer_properties))?;
  Ok(OutputWriter {
    writer: Some(writer),
    rows_written: 0,
  })
}

/// Append one record batch and update the writer's row count.
pub fn write_batches(writer: &mut OutputWriter, batch: &RecordBatch) -> Result<()> {
  writer
    .writer
    .as_mut()
    .context("missing parquet writer")?
    .write(batch)?;
  writer.rows_written += batch.num_rows() as u64;
  Ok(())
}

/// Build DataFusion Parquet options equivalent to the direct-writer configuration.
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

/// Attach file metadata and close every direct writer.
pub fn finalize_writers(mut writers: Vec<OutputWriter>, kv_metadata: &[KeyValue]) -> Result<()> {
  for writer in writers.iter_mut() {
    let mut parquet_writer = writer.writer.take().context("missing parquet writer")?;
    for kv in kv_metadata {
      parquet_writer.append_key_value_metadata(kv.clone());
    }
    parquet_writer.close()?;
  }
  Ok(())
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

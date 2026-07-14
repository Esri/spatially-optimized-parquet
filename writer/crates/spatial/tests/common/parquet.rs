use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use arrow_array::RecordBatch;
use arrow_schema::{Schema, SchemaRef};
use datafusion::dataframe::DataFrame;
use datafusion::datasource::file_format::options::ParquetReadOptions;
use datafusion::execution::context::SessionContext;
use datafusion_execution::config::SessionConfig;
use gdal::spatial_ref::SpatialRef;
use parquet::arrow::arrow_reader::{ArrowReaderMetadata, ArrowReaderOptions};
use parquet::arrow::arrow_writer::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::metadata::KeyValue;
use parquet::file::properties::WriterProperties;

pub fn write_parquet(
  path: &Path,
  schema: &SchemaRef,
  batches: &[RecordBatch],
  compression: Compression,
  metadata: &[KeyValue],
) {
  let file = File::create(path).unwrap();
  let properties = WriterProperties::builder()
    .set_compression(compression)
    .build();
  let mut writer = ArrowWriter::try_new(file, schema.clone(), Some(properties)).unwrap();
  for batch in batches {
    writer.write(batch).unwrap();
  }
  for entry in metadata {
    writer.append_key_value_metadata(entry.clone());
  }
  writer.close().unwrap();
}

pub async fn scan_parquet(input_path: &str) -> Result<DataFrame> {
  let session_config = SessionConfig::new().with_collect_statistics(false);
  SessionContext::new_with_config(session_config)
    .read_parquet(input_path, ParquetReadOptions::default())
    .await
    .context("read parquet")
}

pub fn geoparquet_kv(primary_column: &str, geometry_types: &[&str]) -> KeyValue {
  geoparquet_kv_with_epsg(primary_column, geometry_types, 4326)
}

pub fn geoparquet_kv_with_epsg(
  primary_column: &str,
  geometry_types: &[&str],
  epsg: u32,
) -> KeyValue {
  let crs = SpatialRef::from_epsg(epsg).unwrap().to_projjson().unwrap();
  let crs: serde_json::Value = serde_json::from_str(&crs).unwrap();
  let geometry_types = geometry_types
    .iter()
    .map(|item| serde_json::Value::String((*item).to_string()))
    .collect::<Vec<_>>();
  let value = serde_json::json!({
    "version": "1.1.0",
    "primary_column": primary_column,
    "columns": {
      primary_column: {
        "encoding": "WKB",
        "geometry_types": geometry_types,
        "crs": crs
      }
    }
  });
  KeyValue::new("geo".to_string(), Some(value.to_string()))
}

pub fn kv_map(path: &Path) -> HashMap<String, String> {
  let metadata = reader_metadata(path);
  metadata
    .metadata()
    .file_metadata()
    .key_value_metadata()
    .map_or(&[][..], |items| items.as_slice())
    .iter()
    .filter_map(|entry| {
      entry
        .value
        .as_ref()
        .map(|value| (entry.key.clone(), value.clone()))
    })
    .collect()
}

pub fn raw_parquet_schema(path: &Path) -> Arc<Schema> {
  Arc::new(reader_metadata(path).schema().as_ref().clone())
}

pub fn parquet_files(path: &Path) -> Vec<PathBuf> {
  let mut files = Vec::new();
  for entry in std::fs::read_dir(path).unwrap() {
    let entry_path = entry.unwrap().path();
    if entry_path.is_dir() {
      files.extend(parquet_files(&entry_path));
    } else if entry_path
      .extension()
      .is_some_and(|extension| extension == "parquet")
    {
      files.push(entry_path);
    }
  }
  files.sort();
  files
}

pub fn reader_metadata(path: &Path) -> ArrowReaderMetadata {
  ArrowReaderMetadata::load(&File::open(path).unwrap(), ArrowReaderOptions::new()).unwrap()
}

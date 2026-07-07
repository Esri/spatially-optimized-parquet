use std::sync::Arc;

use arrow_array::{Int32Array, RecordBatch};
use arrow_schema::{DataType, Field, Schema};
use parquet::basic::Compression;
use parquet::file::metadata::KeyValue;
use parquet::file::reader::FileReader;
use parquet::file::serialized_reader::SerializedFileReader;
use tempfile::TempDir;

use engine::write::{
  create_datafusion_parquet_options, create_output_writer, finalize_writers, parse_compression,
  write_batches,
};

#[test]
fn parse_compression_accepts_known_codecs() {
  let codec = parse_compression("snappy").unwrap();
  assert!(matches!(codec, Compression::SNAPPY));
  let codec = parse_compression("gzip").unwrap();
  assert!(matches!(codec, Compression::GZIP(_)));
  let codec = parse_compression("uncompressed").unwrap();
  assert!(matches!(codec, Compression::UNCOMPRESSED));
}

#[test]
fn parse_compression_rejects_invalid() {
  let err = parse_compression("bogus").unwrap_err();
  assert!(err.to_string().contains("compression"));
}

#[test]
fn write_and_finalize_appends_kv_metadata() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("out.parquet");
  let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int32, false)]));
  let batch =
    RecordBatch::try_new(schema.clone(), vec![Arc::new(Int32Array::from(vec![1, 2]))]).unwrap();
  let mut writer = create_output_writer(&path, &schema, Compression::SNAPPY).unwrap();
  write_batches(&mut writer, &batch).unwrap();

  let kv = vec![KeyValue::new("geo".to_string(), Some("{}".to_string()))];
  finalize_writers(vec![writer], &kv).unwrap();

  let reader = SerializedFileReader::new(std::fs::File::open(&path).unwrap()).unwrap();
  let metadata = reader.metadata();
  let kv_out = metadata.file_metadata().key_value_metadata().unwrap();
  assert!(kv_out.iter().any(|item| item.key == "geo"));
}

#[test]
fn write_batches_counts_multiple_batches() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("out.parquet");
  let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int32, false)]));
  let batch =
    RecordBatch::try_new(schema.clone(), vec![Arc::new(Int32Array::from(vec![1, 2]))]).unwrap();
  let mut writer = create_output_writer(&path, &schema, Compression::SNAPPY).unwrap();
  write_batches(&mut writer, &batch).unwrap();
  write_batches(&mut writer, &batch).unwrap();
  assert_eq!(writer.rows_written, 4);
  finalize_writers(vec![writer], &[]).unwrap();
}

#[test]
fn finalize_writers_allows_empty_metadata() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("out.parquet");
  let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int32, false)]));
  let batch =
    RecordBatch::try_new(schema.clone(), vec![Arc::new(Int32Array::from(vec![1]))]).unwrap();
  let mut writer = create_output_writer(&path, &schema, Compression::SNAPPY).unwrap();
  write_batches(&mut writer, &batch).unwrap();
  finalize_writers(vec![writer], &[]).unwrap();

  let reader = SerializedFileReader::new(std::fs::File::open(&path).unwrap()).unwrap();
  let metadata = reader.metadata();
  assert!(metadata.file_metadata().key_value_metadata().is_some());
}

#[test]
fn create_datafusion_parquet_options_preserves_writer_tuning_and_metadata() {
  let options = create_datafusion_parquet_options(
    Compression::GZIP(parquet::basic::GzipLevel::default()),
    &[KeyValue::new("geo".to_string(), Some("{}".to_string()))],
  );

  assert_eq!(options.global.compression.as_deref(), Some("gzip(6)"));
  assert_eq!(options.global.dictionary_enabled, Some(false));
  assert_eq!(
    options
      .key_value_metadata
      .get("geo")
      .and_then(|value| value.as_deref()),
    Some("{}")
  );
}

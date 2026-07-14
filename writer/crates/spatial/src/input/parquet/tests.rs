use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use arrow_array::{BinaryArray, Int32Array, RecordBatch};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use futures_util::StreamExt;
use parquet::arrow::arrow_writer::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::metadata::KeyValue;
use parquet::file::properties::WriterProperties;
use tempfile::TempDir;
use tokio::runtime::Runtime;

use crate::input::{InputOpenOptions, RowRange, SourceFormat, open_input};
use crate::session::DataFusionSession;

fn runtime() -> Runtime {
  Runtime::new().unwrap()
}

fn sample_schema_with_geometry() -> SchemaRef {
  Arc::new(Schema::new(vec![
    Field::new("id", DataType::Int32, false),
    Field::new("geometry", DataType::Binary, true),
  ]))
}

fn sample_batch_with_geometry(wkb_values: Vec<Option<Vec<u8>>>) -> RecordBatch {
  let ids = Int32Array::from_iter_values(1..=wkb_values.len() as i32);
  let values = wkb_values
    .iter()
    .map(|value| value.as_deref())
    .collect::<Vec<_>>();
  RecordBatch::try_new(
    sample_schema_with_geometry(),
    vec![Arc::new(ids), Arc::new(BinaryArray::from(values))],
  )
  .unwrap()
}

fn write_parquet(
  path: &Path,
  schema: &SchemaRef,
  batches: &[RecordBatch],
  compression: Compression,
  metadata: &[KeyValue],
) {
  let properties = WriterProperties::builder()
    .set_compression(compression)
    .build();
  let mut writer = ArrowWriter::try_new(
    File::create(path).unwrap(),
    schema.clone(),
    Some(properties),
  )
  .unwrap();
  for batch in batches {
    writer.write(batch).unwrap();
  }
  for entry in metadata {
    writer.append_key_value_metadata(entry.clone());
  }
  writer.close().unwrap();
}

fn string_value(array: &dyn arrow_array::Array, index: usize) -> String {
  if let Some(array) = array.as_any().downcast_ref::<arrow_array::StringArray>() {
    return array.value(index).to_string();
  }
  if let Some(array) = array
    .as_any()
    .downcast_ref::<arrow_array::StringViewArray>()
  {
    return array.value(index).to_string();
  }
  panic!("unexpected string array type: {:?}", array.data_type());
}

#[test]
fn open_input_accepts_single_parquet_file() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("data.parquet");
  let schema = sample_schema_with_geometry();
  let batch = sample_batch_with_geometry(vec![None, None, None]);
  write_parquet(
    &path,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[],
  );

  let input = runtime()
    .block_on(open_input(
      SourceFormat::Parquet,
      &InputOpenOptions::new(path.to_string_lossy().into_owned(), None),
    ))
    .unwrap();
  assert_eq!(input.total_rows().unwrap(), 3);
}

#[test]
fn open_input_accepts_directory_of_parquet_files() {
  let temp = TempDir::new().unwrap();
  let schema = sample_schema_with_geometry();
  let batch = sample_batch_with_geometry(vec![None, None, None]);
  write_parquet(
    &temp.path().join("b.parquet"),
    &schema,
    std::slice::from_ref(&batch),
    parquet::basic::Compression::SNAPPY,
    &[],
  );
  write_parquet(
    &temp.path().join("a.parquet"),
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[],
  );

  let input = runtime()
    .block_on(open_input(
      SourceFormat::Parquet,
      &InputOpenOptions::new(temp.path().to_string_lossy().into_owned(), None),
    ))
    .unwrap();
  assert_eq!(input.total_rows().unwrap(), 6);
}

#[test]
fn open_input_rejects_non_parquet_file() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("data.txt");
  std::fs::write(&path, "hi").unwrap();

  let err = match runtime().block_on(open_input(
    SourceFormat::Parquet,
    &InputOpenOptions::new(path.to_string_lossy().into_owned(), None),
  )) {
    Ok(_) => panic!("expected non-parquet input to be rejected"),
    Err(err) => err,
  };
  assert!(err.to_string().contains("parquet input must be"));
}

#[test]
fn input_schema_and_batch_limit_work() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("data.parquet");
  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(arrow_array::StringArray::from(vec!["a", "b", "c"])),
      Arc::new(arrow_array::BinaryArray::from(vec![None, None, None])),
    ],
  )
  .unwrap();
  write_parquet(
    &path,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[],
  );

  let input = runtime()
    .block_on(open_input(
      SourceFormat::Parquet,
      &InputOpenOptions::new(path.to_string_lossy().into_owned(), None),
    ))
    .unwrap();
  let schema = input.schema().unwrap();
  assert!(schema.field_with_name("name").is_ok());
  assert!(schema.field_with_name("geometry").is_ok());

  let rows = runtime().block_on(async {
    let mut stream = input.read_batches(RowRange::new(0, Some(2))).await.unwrap();
    let mut rows = 0usize;
    while let Some(batch) = stream.next().await {
      rows += batch.unwrap().num_rows();
    }
    rows
  });
  assert_eq!(rows, 2);
}

#[test]
fn parquet_input_can_produce_dataframe_for_execution() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("data.parquet");
  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(arrow_array::StringArray::from(vec!["a", "b", "c"])),
      Arc::new(arrow_array::BinaryArray::from(vec![None, None, None])),
    ],
  )
  .unwrap();
  write_parquet(
    &path,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[],
  );

  let input = runtime()
    .block_on(open_input(
      SourceFormat::Parquet,
      &InputOpenOptions::new(path.to_string_lossy().into_owned(), None),
    ))
    .unwrap();
  let session = DataFusionSession::new().unwrap();

  let rows = runtime().block_on(async {
    let df = input
      .to_dataframe(session.context(), RowRange::new(1, Some(1)))
      .await
      .unwrap();
    let batches = df.collect().await.unwrap();
    let rows = batches.iter().map(|batch| batch.num_rows()).sum::<usize>();
    let name = string_value(batches[0].column_by_name("name").unwrap().as_ref(), 0);
    (rows, name)
  });
  assert_eq!(rows, (1, "b".to_string()));
}

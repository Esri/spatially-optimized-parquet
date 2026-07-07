use std::sync::Arc;

use arrow_array::RecordBatch;
use arrow_schema::{DataType, Field, Schema};
use engine::session::new_datafusion_session;
use futures_util::StreamExt;
use tempfile::TempDir;

use spatial::input::parquet::ParquetInputProvider;
use spatial::input::{InputOpenOptions, InputProvider, RowRange, open_input};

mod common;
use common::{runtime, sample_batch_with_geometry, sample_schema_with_geometry, write_parquet};

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

  let providers: Vec<Box<dyn InputProvider>> = vec![Box::new(ParquetInputProvider::new())];
  let input = runtime()
    .block_on(open_input(&InputOpenOptions::new(path.clone()), &providers))
    .unwrap();
  assert_eq!(input.format_name(), "parquet");
  assert_eq!(input.source_location(), path.to_str().unwrap());
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

  let providers: Vec<Box<dyn InputProvider>> = vec![Box::new(ParquetInputProvider::new())];
  let input = runtime()
    .block_on(open_input(
      &InputOpenOptions::new(temp.path().to_path_buf()),
      &providers,
    ))
    .unwrap();
  assert_eq!(input.format_name(), "parquet");
  assert_eq!(input.total_rows().unwrap(), 6);
}

#[test]
fn open_input_rejects_non_parquet_file() {
  let temp = TempDir::new().unwrap();
  let path = temp.path().join("data.txt");
  std::fs::write(&path, "hi").unwrap();

  let providers: Vec<Box<dyn InputProvider>> = vec![Box::new(ParquetInputProvider::new())];
  let err = match runtime().block_on(open_input(&InputOpenOptions::new(path.clone()), &providers)) {
    Ok(_) => panic!("expected non-parquet input to be rejected"),
    Err(err) => err,
  };
  assert!(
    err
      .to_string()
      .contains("no input provider could open input")
  );
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

  let providers: Vec<Box<dyn InputProvider>> = vec![Box::new(ParquetInputProvider::new())];
  let input = runtime()
    .block_on(open_input(&InputOpenOptions::new(path.clone()), &providers))
    .unwrap();
  let schema = input.schema().unwrap();
  assert!(schema.field_with_name("name").is_ok());
  assert!(schema.field_with_name("geometry").is_ok());

  let rows = runtime().block_on(async {
    let mut stream = input
      .read_batches(RowRange {
        start: 0,
        num: Some(2),
      })
      .await
      .unwrap();
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

  let providers: Vec<Box<dyn InputProvider>> = vec![Box::new(ParquetInputProvider::new())];
  let input = runtime()
    .block_on(open_input(&InputOpenOptions::new(path.clone()), &providers))
    .unwrap();
  let session = new_datafusion_session().unwrap();

  let rows = runtime().block_on(async {
    let df = input
      .to_dataframe(
        session.context(),
        RowRange {
          start: 1,
          num: Some(1),
        },
      )
      .await
      .unwrap();
    let batches = df.collect().await.unwrap();
    let rows = batches.iter().map(|batch| batch.num_rows()).sum::<usize>();
    let name = string_value(batches[0].column_by_name("name").unwrap().as_ref(), 0);
    (rows, name)
  });
  assert_eq!(rows, (1, "b".to_string()));
}

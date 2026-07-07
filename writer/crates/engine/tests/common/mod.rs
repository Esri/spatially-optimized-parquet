use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use arrow_array::{Int32Array, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use parquet::arrow::arrow_writer::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::metadata::KeyValue;
use parquet::file::properties::WriterProperties;

pub fn sample_schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int32, false),
        Field::new("name", DataType::Utf8, true),
    ]))
}

pub fn sample_batch() -> RecordBatch {
    let ids = Int32Array::from(vec![1, 2, 3]);
    let names = StringArray::from(vec![Some("a"), None, Some("c")]);
    RecordBatch::try_new(sample_schema(), vec![Arc::new(ids), Arc::new(names)]).unwrap()
}

pub fn write_parquet(
    path: &Path,
    schema: &SchemaRef,
    batches: &[RecordBatch],
    compression: Compression,
    kv: &[KeyValue],
) {
    let file = File::create(path).unwrap();
    let writer_properties = WriterProperties::builder()
        .set_compression(compression)
        .build();
    let mut writer = ArrowWriter::try_new(file, schema.clone(), Some(writer_properties)).unwrap();
    for batch in batches {
        writer.write(batch).unwrap();
    }
    for kv in kv {
        writer.append_key_value_metadata(kv.clone());
    }
    writer.close().unwrap();
}

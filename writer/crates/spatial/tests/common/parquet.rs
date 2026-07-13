use std::fs::File;
use std::path::Path;

use arrow_array::RecordBatch;
use arrow_schema::SchemaRef;
use gdal::spatial_ref::SpatialRef;
use parquet::arrow::arrow_writer::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::metadata::KeyValue;
use parquet::file::properties::WriterProperties;

#[allow(dead_code)]
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

#[allow(dead_code)]
pub fn geoparquet_kv(primary_column: &str, geometry_types: &[&str]) -> KeyValue {
  geoparquet_kv_with_epsg(primary_column, geometry_types, 4326)
}

#[allow(dead_code)]
pub fn geoparquet_kv_with_epsg(
  primary_column: &str,
  geometry_types: &[&str],
  epsg: u32,
) -> KeyValue {
  let crs = SpatialRef::from_epsg(epsg).unwrap().to_projjson().unwrap();
  let crs: serde_json::Value = serde_json::from_str(&crs).unwrap();
  let types = geometry_types
    .iter()
    .map(|item| serde_json::Value::String((*item).to_string()))
    .collect::<Vec<_>>();
  let value = serde_json::json!({
      "version": "1.1.0",
      "primary_column": primary_column,
      "columns": {
          primary_column: {
              "encoding": "WKB",
              "geometry_types": types,
              "crs": crs
          }
      }
  });
  KeyValue::new("geo".to_string(), Some(value.to_string()))
}

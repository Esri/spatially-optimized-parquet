use std::sync::Arc;

use arrow_array::{BinaryArray, Int32Array, RecordBatch};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use wkb::writer::write_geometry;

#[allow(dead_code)]
pub fn sample_schema_with_geometry() -> SchemaRef {
  Arc::new(Schema::new(vec![
    Field::new("id", DataType::Int32, false),
    Field::new("geometry", DataType::Binary, true),
  ]))
}

#[allow(dead_code)]
pub fn sample_batch_with_geometry(wkb_values: Vec<Option<Vec<u8>>>) -> RecordBatch {
  let ids = Int32Array::from(vec![1, 2, 3]);
  let values: Vec<Option<&[u8]>> = wkb_values.iter().map(|value| value.as_deref()).collect();
  let geom = BinaryArray::from(values);
  RecordBatch::try_new(
    sample_schema_with_geometry(),
    vec![Arc::new(ids), Arc::new(geom)],
  )
  .unwrap()
}

#[allow(dead_code)]
pub fn wkb_point(x: f64, y: f64) -> Vec<u8> {
  let geom = geo::Geometry::Point(geo::Point::new(x, y));
  let mut buf = Vec::new();
  write_geometry(&mut buf, &geom, &Default::default()).unwrap();
  buf
}

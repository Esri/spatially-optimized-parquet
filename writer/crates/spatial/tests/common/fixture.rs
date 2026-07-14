use std::sync::Arc;

use arrow_array::{BinaryArray, Int32Array, RecordBatch};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use wkb::writer::{WriteOptions, write_geometry};

pub fn sample_schema_with_geometry() -> SchemaRef {
  Arc::new(Schema::new(vec![
    Field::new("id", DataType::Int32, false),
    Field::new("geometry", DataType::Binary, true),
  ]))
}

pub fn sample_batch_with_geometry(wkb_values: Vec<Option<Vec<u8>>>) -> RecordBatch {
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

pub fn wkb_point(x: f64, y: f64) -> Vec<u8> {
  let geometry = geo::Geometry::Point(geo::Point::new(x, y));
  let mut buffer = Vec::new();
  write_geometry(&mut buffer, &geometry, &WriteOptions::default()).unwrap();
  buffer
}

pub fn wkb_polygon(coords: &[(f64, f64)]) -> Vec<u8> {
  let geometry = geo::Geometry::Polygon(geo::Polygon::new(
    geo::LineString::from(coords.to_vec()),
    vec![],
  ));
  let mut buffer = Vec::new();
  write_geometry(&mut buffer, &geometry, &WriteOptions::default()).unwrap();
  buffer
}

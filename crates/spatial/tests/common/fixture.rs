use std::sync::Arc;

use arrow_array::{BinaryArray, Int32Array, RecordBatch};
use arrow_schema::{DataType, Field, Schema, SchemaRef};

use super::wkb;

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
  wkb::point(x, y)
}

pub fn wkb_dimensional_point(x: f64, y: f64, z: Option<f64>, m: Option<f64>) -> Vec<u8> {
  wkb::dimensional_point(x, y, z, m)
}

pub fn wkb_dimensional_multi_point(coordinates: &[wkb::DimensionalCoordinate]) -> Vec<u8> {
  wkb::dimensional_multi_point(coordinates)
}

pub fn wkb_dimensional_line_string(coordinates: &[wkb::DimensionalCoordinate]) -> Vec<u8> {
  wkb::dimensional_line_string(coordinates)
}

pub fn wkb_polygon(coords: &[(f64, f64)]) -> Vec<u8> {
  wkb::polygon(coords)
}

pub fn wkb_dimensional_polygon(coordinates: &[(f64, f64, Option<f64>, Option<f64>)]) -> Vec<u8> {
  wkb::dimensional_polygon(coordinates)
}

pub fn wkb_dimensional_polygon_rings(rings: &[&[wkb::DimensionalCoordinate]]) -> Vec<u8> {
  wkb::dimensional_polygon_rings(rings)
}

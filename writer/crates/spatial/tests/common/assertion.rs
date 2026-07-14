use arrow_array::{
  Array, BinaryArray, BinaryViewArray, Float64Array, LargeBinaryArray, StringArray,
  StringViewArray, StructArray,
};

pub fn string_value(array: &dyn Array, index: usize) -> String {
  if let Some(array) = array.as_any().downcast_ref::<StringArray>() {
    return array.value(index).to_string();
  }
  if let Some(array) = array.as_any().downcast_ref::<StringViewArray>() {
    return array.value(index).to_string();
  }
  panic!("unexpected string array type: {:?}", array.data_type());
}

pub fn binary_value(array: &dyn Array, index: usize) -> Vec<u8> {
  if let Some(array) = array.as_any().downcast_ref::<BinaryArray>() {
    return array.value(index).to_vec();
  }
  if let Some(array) = array.as_any().downcast_ref::<LargeBinaryArray>() {
    return array.value(index).to_vec();
  }
  if let Some(array) = array.as_any().downcast_ref::<BinaryViewArray>() {
    return array.value(index).to_vec();
  }
  panic!("unexpected binary array type: {:?}", array.data_type());
}

pub fn struct_f64_value(array: &StructArray, field_name: &str, index: usize) -> f64 {
  array
    .column_by_name(field_name)
    .unwrap()
    .as_any()
    .downcast_ref::<Float64Array>()
    .unwrap()
    .value(index)
}

pub fn assert_close(actual: f64, expected: f64) {
  assert!(
    (actual - expected).abs() < 1.0e-5,
    "expected {expected}, got {actual}"
  );
}

pub fn assert_covering_metadata(geo: &serde_json::Value) {
  let covering = &geo["columns"]["geometry"]["covering"]["bbox"];
  assert_eq!(covering["xmin"], serde_json::json!(["bbox", "xmin"]));
  assert_eq!(covering["ymin"], serde_json::json!(["bbox", "ymin"]));
  assert_eq!(covering["xmax"], serde_json::json!(["bbox", "xmax"]));
  assert_eq!(covering["ymax"], serde_json::json!(["bbox", "ymax"]));
}

pub fn assert_json_extent(extent: &serde_json::Value, expected: [f64; 4]) {
  if let Some(values) = extent.as_array() {
    for (actual, expected) in values.iter().zip(expected) {
      assert_close(actual.as_f64().unwrap(), expected);
    }
    return;
  }
  assert_close(extent["xmin"].as_f64().unwrap(), expected[0]);
  assert_close(extent["ymin"].as_f64().unwrap(), expected[1]);
  assert_close(extent["xmax"].as_f64().unwrap(), expected[2]);
  assert_close(extent["ymax"].as_f64().unwrap(), expected[3]);
}

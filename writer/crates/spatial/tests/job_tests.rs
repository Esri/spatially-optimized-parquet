use std::collections::HashMap;
use std::sync::Arc;

use arrow_array::{
  BinaryArray, BinaryViewArray, Float64Array, Int32Array, LargeBinaryArray, RecordBatch,
  StringArray, StringViewArray, StructArray, UInt64Array,
};
use arrow_schema::{DataType, Field, Schema};
use engine::read::read_parquet_df;
use gdal_sys::OGRwkbGeometryType;
use parquet::arrow::arrow_reader::{ArrowReaderMetadata, ArrowReaderOptions};
use parquet::file::metadata::KeyValue;
use tempfile::TempDir;
use tokio::runtime::Runtime;

use spatial::input::RowRange;
use spatial::job::{OptimizeJobOptions, run_optimize_job};
use spatial::output::GeoParquetOutputMode;
use spatial::output::optimized::multiscale::{geometry_extent_from_wkb, point_xy_from_wkb};
use wkb::writer::WriteOptions;

mod common;
use common::{
  GpkgFeature, GpkgLayerSpec, geoparquet_kv, geoparquet_kv_with_epsg, transform_point_between_epsg,
  wkb_point, write_gpkg, write_parquet,
};

fn runtime() -> Runtime {
  Runtime::new().unwrap()
}

fn kv_map(path: &std::path::Path) -> HashMap<String, String> {
  let metadata = ArrowReaderMetadata::load(
    &std::fs::File::open(path).unwrap(),
    ArrowReaderOptions::new(),
  )
  .unwrap();
  metadata
    .metadata()
    .file_metadata()
    .key_value_metadata()
    .map_or(&[][..], |items| items.as_slice())
    .iter()
    .filter_map(|kv| {
      kv.value
        .as_ref()
        .map(|value| (kv.key.clone(), value.clone()))
    })
    .collect()
}

fn wkb_polygon(coords: &[(f64, f64)]) -> Vec<u8> {
  let polygon = geo::Geometry::Polygon(geo::Polygon::new(
    geo::LineString::from(coords.to_vec()),
    vec![],
  ));
  let mut buffer = Vec::new();
  wkb::writer::write_geometry(&mut buffer, &polygon, &WriteOptions::default()).unwrap();
  buffer
}

fn string_value(array: &dyn arrow_array::Array, index: usize) -> String {
  if let Some(array) = array.as_any().downcast_ref::<StringArray>() {
    return array.value(index).to_string();
  }
  if let Some(array) = array.as_any().downcast_ref::<StringViewArray>() {
    return array.value(index).to_string();
  }
  panic!("unexpected string array type: {:?}", array.data_type());
}

fn binary_value(array: &dyn arrow_array::Array, index: usize) -> Vec<u8> {
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

fn assert_close(actual: f64, expected: f64) {
  assert!(
    (actual - expected).abs() < 1.0e-5,
    "expected {expected}, got {actual}"
  );
}

fn struct_f64_value(array: &StructArray, field_name: &str, index: usize) -> f64 {
  array
    .column_by_name(field_name)
    .unwrap()
    .as_any()
    .downcast_ref::<Float64Array>()
    .unwrap()
    .value(index)
}

fn assert_covering_metadata(geo: &serde_json::Value) {
  let covering = &geo["columns"]["geometry"]["covering"]["bbox"];
  assert_eq!(covering["xmin"], serde_json::json!(["bbox", "xmin"]));
  assert_eq!(covering["ymin"], serde_json::json!(["bbox", "ymin"]));
  assert_eq!(covering["xmax"], serde_json::json!(["bbox", "xmax"]));
  assert_eq!(covering["ymax"], serde_json::json!(["bbox", "ymax"]));
}

#[test]
fn optimize_job_writes_sorted_point_output_and_metadata() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points.parquet");
  let output = temp.path().join("points-optimized.parquet");

  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let point_late = wkb_point(8.0, 8.0);
  let point_early = wkb_point(1.0, 1.0);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(vec!["late", "early"])),
      Arc::new(BinaryArray::from(vec![
        Some(point_late.as_slice()),
        Some(point_early.as_slice()),
      ])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Point"])],
  );

  runtime()
    .block_on(run_optimize_job(OptimizeJobOptions {
      input: input.to_string_lossy().into_owned(),
      input_format: None,
      output: output.clone(),
      output_files: None,
      compression: None,
      row_range: RowRange::default(),
      layer: None,
      geometry_column: None,
      input_wkid: None,
      covering: false,
      overwrite: true,
      progress: false,
      explain: false,
      output_mode: GeoParquetOutputMode::Optimized,
    }))
    .unwrap();

  let df = runtime()
    .block_on(read_parquet_df(output.to_str().unwrap()))
    .unwrap();
  let output_schema = Arc::new(df.schema().as_arrow().clone());
  let batches = runtime().block_on(df.collect()).unwrap();
  let batch = &batches[0];

  assert!(output_schema.index_of("zCode").is_ok());
  assert!(output_schema.index_of("x").is_ok());
  assert!(output_schema.index_of("y").is_ok());
  assert_eq!(
    string_value(batch.column_by_name("name").unwrap().as_ref(), 0),
    "early"
  );

  let kv = kv_map(&output);
  assert!(kv.contains_key("geo"));
  let geo: serde_json::Value = serde_json::from_str(kv.get("geo").unwrap()).unwrap();
  let geodisplay: serde_json::Value = serde_json::from_str(kv.get("geodisplay").unwrap()).unwrap();
  assert_eq!(geodisplay["index"]["type"], "z");
  assert_eq!(geodisplay["index"]["xColumn"], "x");
  assert_eq!(geodisplay["index"]["yColumn"], "y");
  assert_eq!(geodisplay["index"]["wkid"], 4326);
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["authority"], "EPSG");
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["code"], 4326);
}

#[test]
fn optimize_job_writes_covering_bbox_for_reprojected_points() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points-3857.parquet");
  let output = temp.path().join("points-covering.parquet");

  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let (mx, my) = transform_point_between_epsg(1.0, 1.0, 4326, 3857);
  let projected_point = wkb_point(mx, my);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(vec!["projected"])),
      Arc::new(BinaryArray::from(vec![Some(projected_point.as_slice())])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv_with_epsg("geometry", &["Point"], 3857)],
  );

  runtime()
    .block_on(run_optimize_job(OptimizeJobOptions {
      input: input.to_string_lossy().into_owned(),
      input_format: None,
      output: output.clone(),
      output_files: None,
      compression: None,
      row_range: RowRange::default(),
      layer: None,
      geometry_column: None,
      input_wkid: None,
      covering: true,
      overwrite: true,
      progress: false,
      explain: false,
      output_mode: GeoParquetOutputMode::Optimized,
    }))
    .unwrap();

  let df = runtime()
    .block_on(read_parquet_df(output.to_str().unwrap()))
    .unwrap();
  let output_schema = Arc::new(df.schema().as_arrow().clone());
  assert!(output_schema.index_of("bbox").is_ok());
  let batches = runtime().block_on(df.collect()).unwrap();
  let bbox = batches[0]
    .column_by_name("bbox")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();

  assert_close(struct_f64_value(bbox, "xmin", 0), 1.0);
  assert_close(struct_f64_value(bbox, "ymin", 0), 1.0);
  assert_close(struct_f64_value(bbox, "xmax", 0), 1.0);
  assert_close(struct_f64_value(bbox, "ymax", 0), 1.0);

  let kv = kv_map(&output);
  let geo: serde_json::Value = serde_json::from_str(kv.get("geo").unwrap()).unwrap();
  assert_covering_metadata(&geo);
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["code"], 4326);
}

#[test]
fn optimize_job_reprojects_geoparquet_point_output_to_wgs84() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points-3857.parquet");
  let output = temp.path().join("points-optimized.parquet");

  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let (mx, my) = transform_point_between_epsg(1.0, 1.0, 4326, 3857);
  let projected_point = wkb_point(mx, my);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(vec!["projected"])),
      Arc::new(BinaryArray::from(vec![Some(projected_point.as_slice())])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv_with_epsg("geometry", &["Point"], 3857)],
  );

  runtime()
    .block_on(run_optimize_job(OptimizeJobOptions {
      input: input.to_string_lossy().into_owned(),
      input_format: None,
      output: output.clone(),
      output_files: None,
      compression: None,
      row_range: RowRange::default(),
      layer: None,
      geometry_column: None,
      input_wkid: None,
      covering: false,
      overwrite: true,
      progress: false,
      explain: false,
      output_mode: GeoParquetOutputMode::Optimized,
    }))
    .unwrap();

  let df = runtime()
    .block_on(read_parquet_df(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(df.collect()).unwrap();
  let batch = &batches[0];
  let x = batch
    .column_by_name("x")
    .unwrap()
    .as_any()
    .downcast_ref::<arrow_array::Float64Array>()
    .unwrap();
  let y = batch
    .column_by_name("y")
    .unwrap()
    .as_any()
    .downcast_ref::<arrow_array::Float64Array>()
    .unwrap();
  assert_close(x.value(0), 1.0);
  assert_close(y.value(0), 1.0);

  let geometry = binary_value(batch.column_by_name("geometry").unwrap().as_ref(), 0);
  let (out_x, out_y) = point_xy_from_wkb(&geometry).unwrap();
  assert_close(out_x, 1.0);
  assert_close(out_y, 1.0);

  let kv = kv_map(&output);
  let geo: serde_json::Value = serde_json::from_str(kv.get("geo").unwrap()).unwrap();
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["code"], 4326);
  let bbox = geo["columns"]["geometry"]["bbox"].as_array().unwrap();
  assert_close(bbox[0].as_f64().unwrap(), 1.0);
  assert_close(bbox[1].as_f64().unwrap(), 1.0);
  assert_close(bbox[2].as_f64().unwrap(), 1.0);
  assert_close(bbox[3].as_f64().unwrap(), 1.0);
}

#[test]
fn optimize_job_writes_non_point_display_struct_and_metadata() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("polygons.parquet");
  let output = temp.path().join("polygons-optimized.parquet");

  let schema = Arc::new(Schema::new(vec![
    Field::new("id", DataType::Int32, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let polygon_late = wkb_polygon(&[(7.0, 7.0), (9.0, 7.0), (9.0, 9.0), (7.0, 7.0)]);
  let polygon_early = wkb_polygon(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 0.0)]);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(Int32Array::from(vec![2, 1])),
      Arc::new(BinaryArray::from(vec![
        Some(polygon_late.as_slice()),
        Some(polygon_early.as_slice()),
      ])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Polygon"])],
  );

  runtime()
    .block_on(run_optimize_job(OptimizeJobOptions {
      input: input.to_string_lossy().into_owned(),
      input_format: None,
      output: output.clone(),
      output_files: None,
      compression: None,
      row_range: RowRange::default(),
      layer: None,
      geometry_column: None,
      input_wkid: None,
      covering: false,
      overwrite: true,
      progress: false,
      explain: false,
      output_mode: GeoParquetOutputMode::Optimized,
    }))
    .unwrap();

  let df = runtime()
    .block_on(read_parquet_df(output.to_str().unwrap()))
    .unwrap();
  let output_schema = Arc::new(df.schema().as_arrow().clone());
  let batches = runtime().block_on(df.collect()).unwrap();
  let batch = &batches[0];
  let ids = batch
    .column_by_name("id")
    .unwrap()
    .as_any()
    .downcast_ref::<Int32Array>()
    .unwrap();
  let display = batch
    .column_by_name("geodisplay")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();

  assert!(output_schema.index_of("geodisplay").is_ok());
  assert_eq!(ids.value(0), 1);
  assert!(display.column_by_name("xzCode").is_some());
  assert!(display.column_by_name("bounds").is_some());

  let kv = kv_map(&output);
  let geo: serde_json::Value = serde_json::from_str(kv.get("geo").unwrap()).unwrap();
  let geodisplay: serde_json::Value = serde_json::from_str(kv.get("geodisplay").unwrap()).unwrap();
  assert_eq!(geodisplay["parentColumn"], "geodisplay");
  assert_eq!(geodisplay["index"]["type"], "xz");
  assert_eq!(geodisplay["index"]["encoding"], "esriPBF");
  assert_eq!(geodisplay["index"]["wkid"], 4326);
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["authority"], "EPSG");
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["code"], 4326);
  let levels = geodisplay["index"]["levels"].as_array().unwrap();
  assert_eq!(levels.len(), 9);
  assert_eq!(levels[0]["level"], 0);
  assert_eq!(levels[0]["resolution"], 0.70312359375);
  assert_eq!(levels[0]["scale"], 295828763.7958547);
  assert_eq!(levels[0]["transform"]["scale"][0], 0.70312359375);
  assert_eq!(levels[0]["transform"]["scale"][1], 0.70312359375);
  assert_eq!(levels[0]["transform"]["translate"][0], 0.0);
  assert_eq!(levels[0]["transform"]["translate"][1], 0.0);
  assert_eq!(levels[1]["level"], 2);
  assert_eq!(levels[1]["resolution"], 0.1757808984375);
  assert_eq!(levels[1]["scale"], 73957190.94896367);
}

#[test]
fn optimize_job_writes_covering_bbox_for_non_point_output() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("polygons.parquet");
  let output = temp.path().join("polygons-covering.parquet");

  let schema = Arc::new(Schema::new(vec![
    Field::new("id", DataType::Int32, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let polygon = wkb_polygon(&[(0.0, 0.0), (2.0, 0.0), (2.0, 3.0), (0.0, 0.0)]);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(Int32Array::from(vec![1])),
      Arc::new(BinaryArray::from(vec![Some(polygon.as_slice())])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Polygon"])],
  );

  runtime()
    .block_on(run_optimize_job(OptimizeJobOptions {
      input: input.to_string_lossy().into_owned(),
      input_format: None,
      output: output.clone(),
      output_files: None,
      compression: None,
      row_range: RowRange::default(),
      layer: None,
      geometry_column: None,
      input_wkid: None,
      covering: true,
      overwrite: true,
      progress: false,
      explain: false,
      output_mode: GeoParquetOutputMode::Optimized,
    }))
    .unwrap();

  let df = runtime()
    .block_on(read_parquet_df(output.to_str().unwrap()))
    .unwrap();
  let output_schema = Arc::new(df.schema().as_arrow().clone());
  assert!(output_schema.index_of("bbox").is_ok());
  assert!(output_schema.index_of("geodisplay").is_ok());
  let batches = runtime().block_on(df.collect()).unwrap();
  let bbox = batches[0]
    .column_by_name("bbox")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();

  assert_close(struct_f64_value(bbox, "xmin", 0), 0.0);
  assert_close(struct_f64_value(bbox, "ymin", 0), 0.0);
  assert_close(struct_f64_value(bbox, "xmax", 0), 2.0);
  assert_close(struct_f64_value(bbox, "ymax", 0), 3.0);

  let kv = kv_map(&output);
  let geo: serde_json::Value = serde_json::from_str(kv.get("geo").unwrap()).unwrap();
  assert_covering_metadata(&geo);
}

#[test]
fn optimize_job_replaces_existing_non_point_geodisplay_column() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("polygons-with-display.parquet");
  let output = temp.path().join("polygons-regenerated.parquet");

  let schema = Arc::new(Schema::new(vec![
    Field::new("id", DataType::Int32, false),
    Field::new("geometry", DataType::Binary, true),
    Field::new("geodisplay", DataType::Utf8, true),
  ]));
  let polygon = wkb_polygon(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 0.0)]);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(Int32Array::from(vec![1])),
      Arc::new(BinaryArray::from(vec![Some(polygon.as_slice())])),
      Arc::new(StringArray::from(vec![Some("stale-display")])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Polygon"])],
  );

  runtime()
    .block_on(run_optimize_job(OptimizeJobOptions {
      input: input.to_string_lossy().into_owned(),
      input_format: None,
      output: output.clone(),
      output_files: None,
      compression: None,
      row_range: RowRange::default(),
      layer: None,
      geometry_column: None,
      input_wkid: None,
      covering: false,
      overwrite: true,
      progress: false,
      explain: false,
      output_mode: GeoParquetOutputMode::Optimized,
    }))
    .unwrap();

  let df = runtime()
    .block_on(read_parquet_df(output.to_str().unwrap()))
    .unwrap();
  let output_schema = Arc::new(df.schema().as_arrow().clone());
  let geodisplay_fields = output_schema
    .fields()
    .iter()
    .filter(|field| field.name() == "geodisplay")
    .count();
  let batches = runtime().block_on(df.collect()).unwrap();
  let display = batches[0]
    .column_by_name("geodisplay")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();

  assert_eq!(geodisplay_fields, 1);
  assert!(display.column_by_name("xzCode").is_some());
  assert!(display.column_by_name("bounds").is_some());
}

#[test]
fn optimize_job_sorts_non_point_rows_across_multiple_input_batches() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("polygons-multi-batch.parquet");
  let output = temp.path().join("polygons-multi-batch-optimized.parquet");

  let schema = Arc::new(Schema::new(vec![
    Field::new("id", DataType::Int32, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let polygon_1 = wkb_polygon(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 0.0)]);
  let polygon_2 = wkb_polygon(&[(2.0, 0.0), (3.0, 0.0), (3.0, 1.0), (2.0, 0.0)]);
  let polygon_3 = wkb_polygon(&[(4.0, 0.0), (5.0, 0.0), (5.0, 1.0), (4.0, 0.0)]);
  let polygon_4 = wkb_polygon(&[(8.0, 0.0), (9.0, 0.0), (9.0, 1.0), (8.0, 0.0)]);
  let batch_a = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(Int32Array::from(vec![4, 2])),
      Arc::new(BinaryArray::from(vec![
        Some(polygon_4.as_slice()),
        Some(polygon_2.as_slice()),
      ])),
    ],
  )
  .unwrap();
  let batch_b = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(Int32Array::from(vec![3, 1])),
      Arc::new(BinaryArray::from(vec![
        Some(polygon_3.as_slice()),
        Some(polygon_1.as_slice()),
      ])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch_a, batch_b],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Polygon"])],
  );

  runtime()
    .block_on(run_optimize_job(OptimizeJobOptions {
      input: input.to_string_lossy().into_owned(),
      input_format: None,
      output: output.clone(),
      output_files: None,
      compression: None,
      row_range: RowRange::default(),
      layer: None,
      geometry_column: None,
      input_wkid: None,
      covering: false,
      overwrite: true,
      progress: false,
      explain: false,
      output_mode: GeoParquetOutputMode::Optimized,
    }))
    .unwrap();

  let df = runtime()
    .block_on(read_parquet_df(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(df.collect()).unwrap();
  let xz_codes: Vec<u64> = batches
    .iter()
    .flat_map(|batch| {
      let display = batch
        .column_by_name("geodisplay")
        .unwrap()
        .as_any()
        .downcast_ref::<StructArray>()
        .unwrap();
      let xz_codes = display
        .column_by_name("xzCode")
        .unwrap()
        .as_any()
        .downcast_ref::<arrow_array::UInt64Array>()
        .unwrap();
      (0..batch.num_rows()).map(move |index| xz_codes.value(index))
    })
    .collect();
  assert!(xz_codes.windows(2).all(|pair| pair[0] <= pair[1]));
}

#[test]
fn optimize_job_writes_range_partitioned_multi_file_output() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points.parquet");
  let output_dir = temp.path().join("out");

  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let point_a = wkb_point(8.0, 8.0);
  let point_b = wkb_point(1.0, 1.0);
  let point_c = wkb_point(4.0, 4.0);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(vec!["late", "early", "middle"])),
      Arc::new(BinaryArray::from(vec![
        Some(point_a.as_slice()),
        Some(point_b.as_slice()),
        Some(point_c.as_slice()),
      ])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Point"])],
  );

  runtime()
    .block_on(run_optimize_job(OptimizeJobOptions {
      input: input.to_string_lossy().into_owned(),
      input_format: None,
      output: output_dir.clone(),
      output_files: Some(2),
      compression: None,
      row_range: RowRange::default(),
      layer: None,
      geometry_column: None,
      input_wkid: None,
      covering: false,
      overwrite: true,
      progress: false,
      explain: false,
      output_mode: GeoParquetOutputMode::Optimized,
    }))
    .unwrap();

  let df = runtime()
    .block_on(read_parquet_df(output_dir.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(df.collect()).unwrap();
  let total_rows: usize = batches.iter().map(|batch| batch.num_rows()).sum();
  assert_eq!(total_rows, 3);
  let range_dirs = std::fs::read_dir(&output_dir)
    .unwrap()
    .map(|entry| entry.unwrap().path())
    .filter(|path| path.is_dir())
    .collect::<Vec<_>>();
  assert_eq!(range_dirs.len(), 2);

  let mut lower_bounds = range_dirs
    .iter()
    .map(|path| {
      let name = path.file_name().unwrap().to_str().unwrap();
      let value = name
        .strip_prefix("z_order=")
        .expect("expected z_order partition directory");
      value.parse::<u64>().unwrap()
    })
    .collect::<Vec<_>>();
  lower_bounds.sort_unstable();
  assert!(lower_bounds.windows(2).all(|pair| pair[0] < pair[1]));

  for range_dir in range_dirs {
    let df = runtime()
      .block_on(read_parquet_df(range_dir.to_str().unwrap()))
      .unwrap();
    let batches = runtime().block_on(df.collect()).unwrap();
    let z_codes: Vec<u64> = batches
      .iter()
      .flat_map(|batch| {
        let z_codes = batch
          .column_by_name("zCode")
          .unwrap()
          .as_any()
          .downcast_ref::<UInt64Array>()
          .unwrap();
        (0..batch.num_rows()).map(move |index| z_codes.value(index))
      })
      .collect();
    assert!(z_codes.windows(2).all(|pair| pair[0] <= pair[1]));
    assert!(std::fs::read_dir(&range_dir).unwrap().any(|entry| {
      entry
        .unwrap()
        .path()
        .extension()
        .is_some_and(|ext| ext == "parquet")
    }));
  }
}

#[test]
fn optimize_job_row_range_writes_requested_input_rows() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points.parquet");
  let output = temp.path().join("points-limited.parquet");

  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let point_a = wkb_point(8.0, 8.0);
  let point_b = wkb_point(1.0, 1.0);
  let point_c = wkb_point(4.0, 4.0);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(vec!["late", "early", "middle"])),
      Arc::new(BinaryArray::from(vec![
        Some(point_a.as_slice()),
        Some(point_b.as_slice()),
        Some(point_c.as_slice()),
      ])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Point"])],
  );

  runtime()
    .block_on(run_optimize_job(OptimizeJobOptions {
      input: input.to_string_lossy().into_owned(),
      input_format: None,
      output: output.clone(),
      output_files: None,
      compression: None,
      row_range: RowRange {
        start: 1,
        num: Some(1),
      },
      layer: None,
      geometry_column: None,
      input_wkid: None,
      covering: false,
      overwrite: true,
      progress: false,
      explain: false,
      output_mode: GeoParquetOutputMode::Optimized,
    }))
    .unwrap();

  let df = runtime()
    .block_on(read_parquet_df(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(df.collect()).unwrap();
  let total_rows: usize = batches.iter().map(|batch| batch.num_rows()).sum();
  assert_eq!(total_rows, 1);

  let names: Vec<String> = batches
    .iter()
    .flat_map(|batch| {
      let column = batch.column_by_name("name").unwrap();
      let len = batch.num_rows();
      (0..len).map(move |index| string_value(column.as_ref(), index))
    })
    .collect();
  assert_eq!(names, vec!["early".to_string()]);
}

#[test]
fn plain_geoparquet_preserves_rows_without_sop_metadata() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("optimized-points.parquet");
  let output = temp.path().join("passthrough.parquet");

  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
    Field::new("zCode", DataType::UInt64, false),
  ]));
  let point_a = wkb_point(8.0, 8.0);
  let point_b = wkb_point(1.0, 1.0);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(vec!["late", "early"])),
      Arc::new(BinaryArray::from(vec![
        Some(point_a.as_slice()),
        Some(point_b.as_slice()),
      ])),
      Arc::new(UInt64Array::from(vec![2, 1])),
    ],
  )
  .unwrap();
  let geodisplay = r#"{"index":{"type":"z","column":"zCode"}}"#.to_string();
  let custom_value = "keep-me".to_string();
  let metadata = vec![
    geoparquet_kv("geometry", &["Point"]),
    KeyValue {
      key: "geodisplay".to_string(),
      value: Some(geodisplay.clone()),
    },
    KeyValue {
      key: "custom".to_string(),
      value: Some(custom_value.clone()),
    },
  ];
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &metadata,
  );

  runtime()
    .block_on(run_optimize_job(OptimizeJobOptions {
      input: input.to_string_lossy().into_owned(),
      input_format: None,
      output: output.clone(),
      output_files: None,
      compression: None,
      row_range: RowRange {
        start: 0,
        num: Some(2),
      },
      layer: None,
      geometry_column: None,
      input_wkid: None,
      covering: false,
      overwrite: true,
      progress: false,
      explain: false,
      output_mode: GeoParquetOutputMode::Plain,
    }))
    .unwrap();

  let df = runtime()
    .block_on(read_parquet_df(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(df.collect()).unwrap();
  let batch = &batches[0];
  let names = (0..batch.num_rows())
    .map(|index| string_value(batch.column_by_name("name").unwrap().as_ref(), index))
    .collect::<Vec<_>>();
  assert_eq!(names, vec!["late".to_string(), "early".to_string()]);

  let z_codes = batch
    .column_by_name("zCode")
    .unwrap()
    .as_any()
    .downcast_ref::<UInt64Array>()
    .unwrap();
  assert_eq!(z_codes.value(0), 2);
  assert_eq!(z_codes.value(1), 1);

  let output_metadata = kv_map(&output);
  assert_eq!(output_metadata.get("geodisplay"), None);
  assert_eq!(output_metadata.get("custom"), Some(&custom_value));
  assert!(output_metadata.get("geo").is_some());
}

#[test]
fn plain_geoparquet_writes_covering_bbox() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points.parquet");
  let output = temp.path().join("points-passthrough.parquet");

  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let point = wkb_point(1.0, 1.0);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(vec!["point"])),
      Arc::new(BinaryArray::from(vec![Some(point.as_slice())])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Point"])],
  );

  runtime()
    .block_on(run_optimize_job(OptimizeJobOptions {
      input: input.to_string_lossy().into_owned(),
      input_format: None,
      output: output.clone(),
      output_files: None,
      compression: None,
      row_range: RowRange::default(),
      layer: None,
      geometry_column: None,
      input_wkid: None,
      covering: true,
      overwrite: true,
      progress: false,
      explain: false,
      output_mode: GeoParquetOutputMode::Plain,
    }))
    .unwrap();
  let dataframe = runtime()
    .block_on(read_parquet_df(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  assert!(batches[0].column_by_name("bbox").is_some());
  let geo: serde_json::Value = serde_json::from_str(kv_map(&output).get("geo").unwrap()).unwrap();
  assert_eq!(
    geo["columns"]["geometry"]["covering"]["bbox"]["xmin"],
    serde_json::json!(["bbox", "xmin"])
  );
}

#[test]
fn optimize_job_rejects_covering_when_bbox_column_exists() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points-with-bbox.parquet");
  let output = temp.path().join("points-covering.parquet");

  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
    Field::new("bbox", DataType::Utf8, true),
  ]));
  let point = wkb_point(1.0, 1.0);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(vec!["point"])),
      Arc::new(BinaryArray::from(vec![Some(point.as_slice())])),
      Arc::new(StringArray::from(vec![Some("existing")])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Point"])],
  );

  let err = runtime()
    .block_on(run_optimize_job(OptimizeJobOptions {
      input: input.to_string_lossy().into_owned(),
      input_format: None,
      output: output.clone(),
      output_files: None,
      compression: None,
      row_range: RowRange::default(),
      layer: None,
      geometry_column: None,
      input_wkid: None,
      covering: true,
      overwrite: true,
      progress: false,
      explain: false,
      output_mode: GeoParquetOutputMode::Optimized,
    }))
    .unwrap_err();
  assert!(
    err
      .to_string()
      .contains("--covering would overwrite existing input column 'bbox'"),
    "{err:#}"
  );
}

#[test]
fn optimize_job_errors_when_explicit_geometry_column_lacks_crs_metadata() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points.parquet");
  let output = temp.path().join("points-optimized.parquet");

  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let point_late = wkb_point(8.0, 8.0);
  let point_early = wkb_point(1.0, 1.0);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(vec!["late", "early"])),
      Arc::new(BinaryArray::from(vec![
        Some(point_late.as_slice()),
        Some(point_early.as_slice()),
      ])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[],
  );

  let err = runtime()
    .block_on(run_optimize_job(OptimizeJobOptions {
      input: input.to_string_lossy().into_owned(),
      input_format: None,
      output: output.clone(),
      output_files: None,
      compression: None,
      row_range: RowRange::default(),
      layer: None,
      geometry_column: Some("geometry".to_string()),
      input_wkid: None,
      covering: false,
      overwrite: true,
      progress: false,
      explain: false,
      output_mode: GeoParquetOutputMode::Optimized,
    }))
    .unwrap_err();
  assert!(err.to_string().contains("pass --in-sr"), "{err:#}");

  runtime()
    .block_on(run_optimize_job(OptimizeJobOptions {
      input: input.to_string_lossy().into_owned(),
      input_format: None,
      output: output.clone(),
      output_files: None,
      compression: None,
      row_range: RowRange::default(),
      layer: None,
      geometry_column: Some("geometry".to_string()),
      input_wkid: Some(3857),
      covering: false,
      overwrite: true,
      progress: false,
      explain: false,
      output_mode: GeoParquetOutputMode::Plain,
    }))
    .unwrap();
  let geo: serde_json::Value = serde_json::from_str(kv_map(&output).get("geo").unwrap()).unwrap();
  assert_eq!(
    geo["columns"]["geometry"]["crs"]["id"]["code"],
    serde_json::json!(3857)
  );
}

#[test]
fn optimize_job_scans_when_geometry_type_metadata_is_missing() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points.parquet");
  let output = temp.path().join("points-optimized.parquet");

  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let point_late = wkb_point(8.0, 8.0);
  let point_early = wkb_point(1.0, 1.0);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(vec!["late", "early"])),
      Arc::new(BinaryArray::from(vec![
        Some(point_late.as_slice()),
        Some(point_early.as_slice()),
      ])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv_with_epsg("geometry", &[], 4326)],
  );

  runtime()
    .block_on(run_optimize_job(OptimizeJobOptions {
      input: input.to_string_lossy().into_owned(),
      input_format: None,
      output: output.clone(),
      output_files: None,
      compression: None,
      row_range: RowRange::default(),
      layer: None,
      geometry_column: None,
      input_wkid: None,
      covering: false,
      overwrite: true,
      progress: false,
      explain: false,
      output_mode: GeoParquetOutputMode::Optimized,
    }))
    .unwrap();
  let metadata = kv_map(&output);
  let geo: serde_json::Value = serde_json::from_str(metadata.get("geo").unwrap()).unwrap();
  assert_eq!(
    geo["columns"]["geometry"]["geometry_types"],
    serde_json::json!(["Point"])
  );
}

#[test]
fn optimize_job_rejects_input_wkid_when_crs_metadata_exists() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points.parquet");
  let output = temp.path().join("points-optimized.parquet");
  let schema = Arc::new(Schema::new(vec![Field::new(
    "geometry",
    DataType::Binary,
    true,
  )]));
  let point = wkb_point(1.0, 1.0);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![Arc::new(BinaryArray::from(vec![Some(point.as_slice())]))],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv_with_epsg("geometry", &["Point"], 4326)],
  );

  let error = runtime()
    .block_on(run_optimize_job(OptimizeJobOptions {
      input: input.to_string_lossy().into_owned(),
      input_format: None,
      output,
      output_files: None,
      compression: None,
      row_range: RowRange::default(),
      layer: None,
      geometry_column: None,
      input_wkid: Some(3857),
      covering: false,
      overwrite: true,
      progress: false,
      explain: false,
      output_mode: GeoParquetOutputMode::Optimized,
    }))
    .unwrap_err();
  assert!(error.to_string().contains("already has CRS metadata"));
}

#[test]
fn optimize_job_accepts_single_layer_geopackage_input() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points.gpkg");
  let output = temp.path().join("points-optimized.parquet");

  let features = [
    GpkgFeature {
      id: 2,
      name: Some("late"),
      geometry_wkt: "POINT (8 8)",
    },
    GpkgFeature {
      id: 1,
      name: Some("early"),
      geometry_wkt: "POINT (1 1)",
    },
  ];
  write_gpkg(
    &input,
    &[GpkgLayerSpec {
      name: "points",
      geometry_type: OGRwkbGeometryType::wkbPoint,
      epsg: Some(4326),
      features: &features,
    }],
  );

  runtime()
    .block_on(run_optimize_job(OptimizeJobOptions {
      input: input.to_string_lossy().into_owned(),
      input_format: None,
      output: output.clone(),
      output_files: None,
      compression: None,
      row_range: RowRange::default(),
      layer: None,
      geometry_column: None,
      input_wkid: None,
      covering: false,
      overwrite: true,
      progress: false,
      explain: false,
      output_mode: GeoParquetOutputMode::Optimized,
    }))
    .unwrap();

  let df = runtime()
    .block_on(read_parquet_df(output.to_str().unwrap()))
    .unwrap();
  let output_schema = Arc::new(df.schema().as_arrow().clone());
  let batches = runtime().block_on(df.collect()).unwrap();
  let batch = &batches[0];

  assert!(output_schema.index_of("zCode").is_ok());
  assert!(output_schema.index_of("x").is_ok());
  assert!(output_schema.index_of("y").is_ok());
  assert_eq!(
    string_value(batch.column_by_name("name").unwrap().as_ref(), 0),
    "early"
  );

  let kv = kv_map(&output);
  let geo: serde_json::Value = serde_json::from_str(kv.get("geo").unwrap()).unwrap();
  assert_eq!(geo["primary_column"], "geometry");
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["authority"], "EPSG");
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["code"], 4326);
}

#[test]
fn optimize_job_reprojects_geopackage_polygon_output_to_wgs84() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("polygons-3857.gpkg");
  let output = temp.path().join("polygons-optimized.parquet");

  let ring = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (0.0, 0.0)];
  let ring_3857 = ring
    .into_iter()
    .map(|(x, y)| transform_point_between_epsg(x, y, 4326, 3857))
    .map(|(x, y)| format!("{x} {y}"))
    .collect::<Vec<_>>()
    .join(", ");
  let polygon_wkt = format!("POLYGON (({ring_3857}))");
  let features = [GpkgFeature {
    id: 1,
    name: Some("projected"),
    geometry_wkt: &polygon_wkt,
  }];
  write_gpkg(
    &input,
    &[GpkgLayerSpec {
      name: "polygons",
      geometry_type: OGRwkbGeometryType::wkbPolygon,
      epsg: Some(3857),
      features: &features,
    }],
  );

  runtime()
    .block_on(run_optimize_job(OptimizeJobOptions {
      input: input.to_string_lossy().into_owned(),
      input_format: None,
      output: output.clone(),
      output_files: None,
      compression: None,
      row_range: RowRange::default(),
      layer: None,
      geometry_column: None,
      input_wkid: None,
      covering: false,
      overwrite: true,
      progress: false,
      explain: false,
      output_mode: GeoParquetOutputMode::Optimized,
    }))
    .unwrap();

  let df = runtime()
    .block_on(read_parquet_df(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(df.collect()).unwrap();
  let batch = &batches[0];
  let geometry = binary_value(batch.column_by_name("geometry").unwrap().as_ref(), 0);
  let extent = geometry_extent_from_wkb(&geometry).unwrap();
  assert_close(extent.xmin, 0.0);
  assert_close(extent.ymin, 0.0);
  assert_close(extent.xmax, 1.0);
  assert_close(extent.ymax, 1.0);

  let display = batch
    .column_by_name("geodisplay")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  let bounds = display
    .column_by_name("bounds")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  let xmin = bounds
    .column_by_name("xmin")
    .unwrap()
    .as_any()
    .downcast_ref::<arrow_array::Float64Array>()
    .unwrap();
  let ymin = bounds
    .column_by_name("ymin")
    .unwrap()
    .as_any()
    .downcast_ref::<arrow_array::Float64Array>()
    .unwrap();
  let xmax = bounds
    .column_by_name("xmax")
    .unwrap()
    .as_any()
    .downcast_ref::<arrow_array::Float64Array>()
    .unwrap();
  let ymax = bounds
    .column_by_name("ymax")
    .unwrap()
    .as_any()
    .downcast_ref::<arrow_array::Float64Array>()
    .unwrap();
  assert_close(xmin.value(0), 0.0);
  assert_close(ymin.value(0), 0.0);
  assert_close(xmax.value(0), 1.0);
  assert_close(ymax.value(0), 1.0);

  let kv = kv_map(&output);
  let geo: serde_json::Value = serde_json::from_str(kv.get("geo").unwrap()).unwrap();
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["code"], 4326);
  let bbox = geo["columns"]["geometry"]["bbox"].as_array().unwrap();
  assert_close(bbox[0].as_f64().unwrap(), 0.0);
  assert_close(bbox[1].as_f64().unwrap(), 0.0);
  assert_close(bbox[2].as_f64().unwrap(), 1.0);
  assert_close(bbox[3].as_f64().unwrap(), 1.0);
}

#[test]
fn optimize_job_selects_requested_geopackage_layer() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("multi.gpkg");
  let output = temp.path().join("polygons-optimized.parquet");

  let point_features = [GpkgFeature {
    id: 10,
    name: Some("point"),
    geometry_wkt: "POINT (0 0)",
  }];
  let polygon_features = [
    GpkgFeature {
      id: 2,
      name: Some("late"),
      geometry_wkt: "POLYGON ((7 7, 9 7, 9 9, 7 7))",
    },
    GpkgFeature {
      id: 1,
      name: Some("early"),
      geometry_wkt: "POLYGON ((0 0, 1 0, 1 1, 0 0))",
    },
  ];
  write_gpkg(
    &input,
    &[
      GpkgLayerSpec {
        name: "points",
        geometry_type: OGRwkbGeometryType::wkbPoint,
        epsg: Some(4326),
        features: &point_features,
      },
      GpkgLayerSpec {
        name: "polygons",
        geometry_type: OGRwkbGeometryType::wkbPolygon,
        epsg: Some(4326),
        features: &polygon_features,
      },
    ],
  );

  runtime()
    .block_on(run_optimize_job(OptimizeJobOptions {
      input: input.to_string_lossy().into_owned(),
      input_format: None,
      output: output.clone(),
      output_files: None,
      compression: None,
      row_range: RowRange::default(),
      layer: Some("polygons".to_string()),
      geometry_column: None,
      input_wkid: None,
      covering: false,
      overwrite: true,
      progress: false,
      explain: false,
      output_mode: GeoParquetOutputMode::Optimized,
    }))
    .unwrap();

  let df = runtime()
    .block_on(read_parquet_df(output.to_str().unwrap()))
    .unwrap();
  let output_schema = Arc::new(df.schema().as_arrow().clone());
  let batches = runtime().block_on(df.collect()).unwrap();
  let batch = &batches[0];
  let ids = batch
    .column_by_name("id")
    .unwrap()
    .as_any()
    .downcast_ref::<Int32Array>()
    .unwrap();
  let display = batch
    .column_by_name("geodisplay")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();

  assert!(output_schema.index_of("geodisplay").is_ok());
  assert_eq!(ids.value(0), 1);
  assert!(display.column_by_name("xzCode").is_some());
  assert!(display.column_by_name("bounds").is_some());

  let kv = kv_map(&output);
  let geo: serde_json::Value = serde_json::from_str(kv.get("geo").unwrap()).unwrap();
  let geodisplay: serde_json::Value = serde_json::from_str(kv.get("geodisplay").unwrap()).unwrap();
  assert_eq!(geo["primary_column"], "geometry");
  assert_eq!(geodisplay["index"]["type"], "xz");
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["authority"], "EPSG");
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["code"], 4326);
}

mod common;

use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use arrow_array::{
  BinaryArray, Float64Array, Int32Array, RecordBatch, StringArray, StructArray, UInt64Array,
};
use arrow_schema::{DataType, Field, Schema};
use gdal_sys::OGRwkbGeometryType;
use spatial::{
  InputOptions, OutputMode, OutputOptions, RowRange, SpatialPipelineOptions, SpatialPipelineResult,
  ValidationRule, WriteProgress, run, validate,
};
use tempfile::TempDir;
use tokio::runtime::Runtime;

use common::assertion::{
  assert_close, assert_covering_metadata, assert_json_extent, binary_value, string_value,
  struct_f64_value,
};
use common::fixture::{wkb_point, wkb_polygon};
use common::geometry::{point_xy_from_wkb, polygon_extent_from_wkb, transform_point_between_epsg};
use common::gpkg::{GpkgFeature, GpkgLayerSpec, write_gpkg};
use common::parquet::{
  geoparquet_kv, geoparquet_kv_with_epsg, kv_map, scan_parquet, write_parquet,
};

fn runtime() -> Runtime {
  Runtime::new().unwrap()
}

fn run_optimized(
  input: &Path,
  output: &Path,
  row_range: RowRange,
  layer: Option<String>,
  geometry_column: Option<String>,
  input_wkid: Option<u32>,
  covering: bool,
) -> Result<SpatialPipelineResult> {
  runtime().block_on(run(SpatialPipelineOptions::new(
    InputOptions::new(
      input.to_string_lossy(),
      None,
      row_range,
      layer,
      geometry_column,
      input_wkid,
    ),
    OutputOptions::new(
      output,
      OutputMode::Optimized,
      None,
      None,
      4326,
      covering,
      true,
    ),
  )))
}

#[test]
fn optimized_output_sorts_points_and_writes_metadata() {
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

  let progress = Arc::new(Mutex::new(Vec::new()));
  let reported = Arc::clone(&progress);
  let result = runtime()
    .block_on(run(
      SpatialPipelineOptions::new(
        InputOptions::new(
          input.to_string_lossy(),
          None,
          RowRange::default(),
          None,
          None,
          None,
        ),
        OutputOptions::new(
          &output,
          OutputMode::Optimized,
          None,
          None,
          4326,
          false,
          true,
        ),
      )
      .with_write_reporter(move |update: WriteProgress| {
        reported.lock().unwrap().push(update);
      }),
    ))
    .unwrap();

  assert_eq!(result.rows_expected(), 2);
  assert_eq!(result.rows_written(), 2);
  let progress = progress.lock().unwrap();
  assert_eq!(progress.last().map(|update| update.rows_written()), Some(2));
  assert_eq!(progress.last().map(|update| update.total_rows()), Some(2));
  let explicit_report = validate(&output).unwrap();
  assert!(!explicit_report.has_errors());
  assert!(
    explicit_report
      .findings()
      .iter()
      .any(|finding| finding.rule() == ValidationRule::WriterMetadata)
  );

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let output_schema = Arc::new(dataframe.schema().as_arrow().clone());
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let batch = &batches[0];
  let geodisplay = output_schema.field_with_name("geodisplay").unwrap();
  assert!(!geodisplay.is_nullable());
  let DataType::Struct(fields) = geodisplay.data_type() else {
    panic!("geodisplay must be a struct");
  };
  assert!(!fields.find("zCode").unwrap().1.is_nullable());
  assert!(!fields.find("x").unwrap().1.is_nullable());
  assert!(!fields.find("y").unwrap().1.is_nullable());
  assert_eq!(
    string_value(batch.column_by_name("name").unwrap().as_ref(), 0),
    "early"
  );
  assert_eq!(
    binary_value(batch.column_by_name("geometry").unwrap().as_ref(), 0),
    point_early
  );

  let metadata = kv_map(&output);
  assert!(metadata.contains_key("geo"));
  let geo: serde_json::Value = serde_json::from_str(metadata.get("geo").unwrap()).unwrap();
  let geodisplay: serde_json::Value =
    serde_json::from_str(metadata.get("geodisplay").unwrap()).unwrap();
  assert_eq!(geodisplay["parentColumn"], "geodisplay");
  assert_eq!(geodisplay["index"]["type"], "z");
  assert_eq!(geodisplay["index"]["version"], "0.1");
  assert_eq!(geodisplay["index"]["geometryType"], "point");
  assert_eq!(geodisplay["index"]["xColumn"], "x");
  assert_eq!(geodisplay["index"]["yColumn"], "y");
  assert_eq!(geodisplay["index"]["wkid"], 4326);
  assert!(
    geodisplay["index"]["wkt"]
      .as_str()
      .unwrap()
      .contains("WGS 84")
  );
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["authority"], "EPSG");
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["code"], 4326);
}

#[test]
fn optimized_output_writes_covering_bbox_for_reprojected_points() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points-3857.parquet");
  let output = temp.path().join("points-covering.parquet");
  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let (x, y) = transform_point_between_epsg(1.0, 1.0, 4326, 3857);
  let projected_point = wkb_point(x, y);
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

  run_optimized(&input, &output, RowRange::default(), None, None, None, true).unwrap();
  assert!(!validate(&output).unwrap().has_errors());

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let output_schema = Arc::new(dataframe.schema().as_arrow().clone());
  assert!(output_schema.index_of("bbox").is_ok());
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let bbox = batches[0]
    .column_by_name("bbox")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  for field in ["xmin", "ymin", "xmax", "ymax"] {
    assert_close(struct_f64_value(bbox, field, 0), 1.0);
  }

  let metadata = kv_map(&output);
  let geo: serde_json::Value = serde_json::from_str(metadata.get("geo").unwrap()).unwrap();
  assert_covering_metadata(&geo);
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["code"], 4326);
}

#[test]
fn optimized_output_reprojects_selected_geoparquet_rows() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points-3857.parquet");
  let output = temp.path().join("points-optimized.parquet");
  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let ignored_xy = transform_point_between_epsg(40.0, 30.0, 4326, 3857);
  let selected_xy = transform_point_between_epsg(1.0, 1.0, 4326, 3857);
  let ignored_point = wkb_point(ignored_xy.0, ignored_xy.1);
  let selected_point = wkb_point(selected_xy.0, selected_xy.1);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(vec!["ignored", "selected"])),
      Arc::new(BinaryArray::from(vec![
        Some(ignored_point.as_slice()),
        Some(selected_point.as_slice()),
      ])),
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

  run_optimized(
    &input,
    &output,
    RowRange::new(1, Some(1)),
    None,
    None,
    None,
    true,
  )
  .unwrap();

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let batch = &batches[0];
  assert_eq!(
    string_value(batch.column_by_name("name").unwrap().as_ref(), 0),
    "selected"
  );
  let geodisplay = batch
    .column_by_name("geodisplay")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  let x = geodisplay
    .column_by_name("x")
    .unwrap()
    .as_any()
    .downcast_ref::<Float64Array>()
    .unwrap();
  let y = geodisplay
    .column_by_name("y")
    .unwrap()
    .as_any()
    .downcast_ref::<Float64Array>()
    .unwrap();
  assert_close(x.value(0), 1.0);
  assert_close(y.value(0), 1.0);
  let geometry = binary_value(batch.column_by_name("geometry").unwrap().as_ref(), 0);
  let (output_x, output_y) = point_xy_from_wkb(&geometry).unwrap();
  assert_close(output_x, 1.0);
  assert_close(output_y, 1.0);

  let metadata = kv_map(&output);
  let geo: serde_json::Value = serde_json::from_str(metadata.get("geo").unwrap()).unwrap();
  let geodisplay: serde_json::Value =
    serde_json::from_str(metadata.get("geodisplay").unwrap()).unwrap();
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["code"], 4326);
  assert_json_extent(&geo["columns"]["geometry"]["bbox"], [1.0, 1.0, 1.0, 1.0]);
  assert_json_extent(&geodisplay["index"]["fullExtent"], [1.0, 1.0, 1.0, 1.0]);
  assert_eq!(geodisplay["index"]["wkid"], 4326);
  assert!(
    geodisplay["index"]["wkt"]
      .as_str()
      .unwrap()
      .contains("WGS 84")
  );
  assert!(!metadata.get("geo").unwrap().contains("3857"));
  assert!(!metadata.get("geodisplay").unwrap().contains("3857"));
}

#[test]
fn optimized_output_writes_non_point_display_struct_and_metadata() {
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
  run_optimized(
    &input,
    &output,
    RowRange::default(),
    None,
    None,
    None,
    false,
  )
  .unwrap();
  let validation = validate(&output).unwrap();
  assert!(
    validation
      .findings()
      .iter()
      .all(|finding| finding.rule() != ValidationRule::PbfWinding)
  );

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let output_schema = Arc::new(dataframe.schema().as_arrow().clone());
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let batch = &batches[0];
  let ids = batch
    .column_by_name("id")
    .unwrap()
    .as_any()
    .downcast_ref::<Int32Array>()
    .unwrap();
  let geodisplay_column = batch
    .column_by_name("geodisplay")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  assert!(output_schema.index_of("geodisplay").is_ok());
  assert_eq!(ids.value(0), 1);
  assert!(geodisplay_column.column_by_name("xzCode").is_some());
  assert!(geodisplay_column.column_by_name("bounds").is_none());

  let metadata = kv_map(&output);
  let geo: serde_json::Value = serde_json::from_str(metadata.get("geo").unwrap()).unwrap();
  let geodisplay: serde_json::Value =
    serde_json::from_str(metadata.get("geodisplay").unwrap()).unwrap();
  assert_eq!(geodisplay["parentColumn"], "geodisplay");
  assert_eq!(geodisplay["index"]["type"], "xz");
  assert_eq!(geodisplay["index"]["version"], "0.1");
  assert_eq!(geodisplay["index"]["encoding"], "esriPBF");
  assert_eq!(geodisplay["index"]["wkid"], 4326);
  assert!(
    geodisplay["index"]["wkt"]
      .as_str()
      .unwrap()
      .contains("WGS 84")
  );
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["authority"], "EPSG");
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["code"], 4326);
  let levels = geodisplay["index"]["levels"].as_array().unwrap();
  assert_eq!(levels.len(), 9);
  assert_eq!(levels[0]["level"], 0);
  assert_eq!(levels[0]["resolution"], 0.703125);
  assert_eq!(levels[0]["scale"], 295829355.4545656);
  assert_eq!(levels[0]["transform"]["scale"][0], 0.703125);
  assert_eq!(levels[0]["transform"]["scale"][1], 0.703125);
  assert_eq!(levels[0]["transform"]["translate"][0], 0.0);
  assert_eq!(levels[0]["transform"]["translate"][1], 0.0);
  assert_eq!(levels[1]["level"], 2);
  assert_eq!(levels[1]["resolution"], 0.17578125);
  assert_eq!(levels[1]["scale"], 73957338.8636414);
}

#[test]
fn optimized_output_writes_covering_bbox_for_non_point_geometry() {
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
  run_optimized(&input, &output, RowRange::default(), None, None, None, true).unwrap();

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let output_schema = Arc::new(dataframe.schema().as_arrow().clone());
  assert!(output_schema.index_of("bbox").is_ok());
  assert!(output_schema.index_of("geodisplay").is_ok());
  let batches = runtime().block_on(dataframe.collect()).unwrap();
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
  let geo: serde_json::Value = serde_json::from_str(kv_map(&output).get("geo").unwrap()).unwrap();
  assert_covering_metadata(&geo);
}

#[test]
fn optimized_output_replaces_existing_display_column() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("polygons-with-geodisplay.parquet");
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
      Arc::new(StringArray::from(vec![Some("stale-geodisplay")])),
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
  run_optimized(
    &input,
    &output,
    RowRange::default(),
    None,
    None,
    None,
    false,
  )
  .unwrap();

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let output_schema = Arc::new(dataframe.schema().as_arrow().clone());
  let geodisplay_fields = output_schema
    .fields()
    .iter()
    .filter(|field| field.name() == "geodisplay")
    .count();
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let geodisplay_column = batches[0]
    .column_by_name("geodisplay")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  assert_eq!(geodisplay_fields, 1);
  assert!(geodisplay_column.column_by_name("xzCode").is_some());
  assert!(geodisplay_column.column_by_name("bounds").is_none());
}

#[test]
fn optimized_output_sorts_non_point_rows_across_input_batches() {
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
  run_optimized(
    &input,
    &output,
    RowRange::default(),
    None,
    None,
    None,
    false,
  )
  .unwrap();

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let xz_codes = batches
    .iter()
    .flat_map(|batch| {
      let geodisplay_column = batch
        .column_by_name("geodisplay")
        .unwrap()
        .as_any()
        .downcast_ref::<StructArray>()
        .unwrap();
      let xz_codes = geodisplay_column
        .column_by_name("xzCode")
        .unwrap()
        .as_any()
        .downcast_ref::<UInt64Array>()
        .unwrap();
      (0..batch.num_rows()).map(move |index| xz_codes.value(index))
    })
    .collect::<Vec<_>>();
  assert!(xz_codes.windows(2).all(|pair| pair[0] <= pair[1]));
}

#[test]
fn optimized_output_applies_requested_row_range() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points.parquet");
  let output = temp.path().join("points-limited.parquet");
  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let point_late = wkb_point(8.0, 8.0);
  let point_early = wkb_point(1.0, 1.0);
  let point_middle = wkb_point(4.0, 4.0);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(vec!["late", "early", "middle"])),
      Arc::new(BinaryArray::from(vec![
        Some(point_late.as_slice()),
        Some(point_early.as_slice()),
        Some(point_middle.as_slice()),
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
  run_optimized(
    &input,
    &output,
    RowRange::new(1, Some(1)),
    None,
    None,
    None,
    false,
  )
  .unwrap();

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(), 1);
  let names = batches
    .iter()
    .flat_map(|batch| {
      let column = batch.column_by_name("name").unwrap();
      (0..batch.num_rows()).map(move |index| string_value(column.as_ref(), index))
    })
    .collect::<Vec<_>>();
  assert_eq!(names, vec!["early".to_string()]);
}

#[test]
fn optimized_output_overwrites_unmanaged_bbox_column() {
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
  run_optimized(&input, &output, RowRange::default(), None, None, None, true).unwrap();

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let bbox = batches[0]
    .column_by_name("bbox")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  assert_close(struct_f64_value(bbox, "xmin", 0), 1.0);
  assert_close(struct_f64_value(bbox, "ymin", 0), 1.0);
}

#[test]
fn optimized_output_scans_geometry_type_when_metadata_is_missing() {
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
  run_optimized(
    &input,
    &output,
    RowRange::default(),
    None,
    None,
    None,
    false,
  )
  .unwrap();

  let metadata = kv_map(&output);
  let geo: serde_json::Value = serde_json::from_str(metadata.get("geo").unwrap()).unwrap();
  assert_eq!(
    geo["columns"]["geometry"]["geometry_types"],
    serde_json::json!(["Point"])
  );
}

#[test]
fn optimized_output_accepts_single_layer_geopackage() {
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
  run_optimized(
    &input,
    &output,
    RowRange::default(),
    None,
    None,
    None,
    false,
  )
  .unwrap();

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let output_schema = Arc::new(dataframe.schema().as_arrow().clone());
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let batch = &batches[0];
  let geodisplay = output_schema.field_with_name("geodisplay").unwrap();
  assert!(matches!(geodisplay.data_type(), DataType::Struct(_)));
  assert_eq!(
    string_value(batch.column_by_name("name").unwrap().as_ref(), 0),
    "early"
  );
  let metadata = kv_map(&output);
  let geo: serde_json::Value = serde_json::from_str(metadata.get("geo").unwrap()).unwrap();
  assert_eq!(geo["primary_column"], "geometry");
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["authority"], "EPSG");
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["code"], 4326);
}

#[test]
fn optimized_output_reprojects_geopackage_polygon() {
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
  run_optimized(&input, &output, RowRange::default(), None, None, None, true).unwrap();

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let batch = &batches[0];
  let geometry = binary_value(batch.column_by_name("geometry").unwrap().as_ref(), 0);
  let extent = polygon_extent_from_wkb(&geometry).unwrap();
  for (actual, expected) in extent.into_iter().zip([0.0, 0.0, 1.0, 1.0]) {
    assert_close(actual, expected);
  }

  let geodisplay_column = batch
    .column_by_name("geodisplay")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  assert!(geodisplay_column.column_by_name("bounds").is_none());
  let level_zero = geodisplay_column.column_by_name("level_0").unwrap();
  assert!(!binary_value(level_zero.as_ref(), 0).is_empty());

  let covering = batch
    .column_by_name("bbox")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  assert_close(struct_f64_value(covering, "xmin", 0), 0.0);
  assert_close(struct_f64_value(covering, "ymin", 0), 0.0);
  assert_close(struct_f64_value(covering, "xmax", 0), 1.0);
  assert_close(struct_f64_value(covering, "ymax", 0), 1.0);

  let metadata = kv_map(&output);
  let geo: serde_json::Value = serde_json::from_str(metadata.get("geo").unwrap()).unwrap();
  let geodisplay: serde_json::Value =
    serde_json::from_str(metadata.get("geodisplay").unwrap()).unwrap();
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["code"], 4326);
  assert_covering_metadata(&geo);
  assert_json_extent(&geo["columns"]["geometry"]["bbox"], [0.0, 0.0, 1.0, 1.0]);
  assert_json_extent(&geodisplay["index"]["fullExtent"], [0.0, 0.0, 1.0, 1.0]);
  assert_eq!(geodisplay["index"]["wkid"], 4326);
  assert!(
    geodisplay["index"]["wkt"]
      .as_str()
      .unwrap()
      .contains("WGS 84")
  );
  assert_eq!(geodisplay["index"]["levels"][0]["column"], "level_0");
  assert_eq!(geodisplay["index"]["levels"][0]["resolution"], 0.703125);
  assert_eq!(
    geodisplay["index"]["levels"][0]["transform"]["scale"][0],
    0.703125
  );
  assert!(!metadata.get("geo").unwrap().contains("3857"));
  assert!(!metadata.get("geodisplay").unwrap().contains("3857"));
}

#[test]
fn optimized_output_selects_requested_geopackage_layer() {
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
  run_optimized(
    &input,
    &output,
    RowRange::default(),
    Some("polygons".to_string()),
    None,
    None,
    false,
  )
  .unwrap();

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let output_schema = Arc::new(dataframe.schema().as_arrow().clone());
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let batch = &batches[0];
  let ids = batch
    .column_by_name("id")
    .unwrap()
    .as_any()
    .downcast_ref::<Int32Array>()
    .unwrap();
  let geodisplay_column = batch
    .column_by_name("geodisplay")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  assert!(output_schema.index_of("geodisplay").is_ok());
  assert_eq!(ids.value(0), 1);
  assert!(geodisplay_column.column_by_name("xzCode").is_some());
  assert!(geodisplay_column.column_by_name("bounds").is_none());

  let metadata = kv_map(&output);
  let geo: serde_json::Value = serde_json::from_str(metadata.get("geo").unwrap()).unwrap();
  let geodisplay: serde_json::Value =
    serde_json::from_str(metadata.get("geodisplay").unwrap()).unwrap();
  assert_eq!(geo["primary_column"], "geometry");
  assert_eq!(geodisplay["parentColumn"], "geodisplay");
  assert_eq!(geodisplay["index"]["type"], "xz");
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["authority"], "EPSG");
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["code"], 4326);
}

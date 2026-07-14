mod common;

use std::sync::{Arc, Mutex};

use arrow_array::{Array, BinaryArray, StringArray, StructArray, UInt64Array};
use arrow_schema::{DataType, Field, Schema};
use parquet::file::metadata::KeyValue;
use spatial::{
  InputOptions, OutputMode, OutputOptions, RowRange, SpatialPipelineOptions, ValidationRule,
  WriteProgress, run, validate,
};
use tempfile::TempDir;
use tokio::runtime::Runtime;

use common::assertion::{
  assert_close, assert_covering_metadata, binary_value, string_value, struct_f64_value,
};
use common::fixture::wkb_point;
use common::geometry::{point_xy_from_wkb, transform_point_between_epsg};
use common::parquet::{
  geoparquet_kv, geoparquet_kv_with_epsg, kv_map, scan_parquet, write_parquet,
};

fn runtime() -> Runtime {
  Runtime::new().unwrap()
}

#[test]
fn plain_output_preserves_wkb_rows_and_passthrough_metadata() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("optimized-points.parquet");
  let output = temp.path().join("passthrough.parquet");
  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
    Field::new("zCode", DataType::UInt64, false),
  ]));
  let point_late = wkb_point(8.0, 8.0);
  let point_early = wkb_point(1.0, 1.0);
  let batch = arrow_array::RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(vec!["late", "early"])),
      Arc::new(BinaryArray::from(vec![
        Some(point_late.as_slice()),
        Some(point_early.as_slice()),
      ])),
      Arc::new(UInt64Array::from(vec![2, 1])),
    ],
  )
  .unwrap();
  let geodisplay = r#"{"index":{"type":"z","column":"zCode"}}"#.to_string();
  let custom_value = "keep-me".to_string();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[
      geoparquet_kv("geometry", &["Point"]),
      KeyValue::new("geodisplay".to_string(), Some(geodisplay)),
      KeyValue::new("custom".to_string(), Some(custom_value.clone())),
    ],
  );

  let progress = Arc::new(Mutex::new(Vec::new()));
  let reported = Arc::clone(&progress);
  let result = runtime()
    .block_on(run(
      SpatialPipelineOptions::new(
        InputOptions::new(
          input.to_string_lossy(),
          None,
          RowRange::new(0, Some(2)),
          None,
          None,
          None,
        ),
        OutputOptions::new(&output, OutputMode::Plain, None, None, 4326, false, true),
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
  assert!(result.validation_report().is_none());
  assert!(output.exists());

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(dataframe.collect()).unwrap();
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
  assert_eq!(
    binary_value(batch.column_by_name("geometry").unwrap().as_ref(), 0),
    point_late
  );

  let output_metadata = kv_map(&output);
  assert_eq!(output_metadata.get("geodisplay"), None);
  assert_eq!(output_metadata.get("custom"), Some(&custom_value));
  assert!(output_metadata.contains_key("geo"));

  let report = validate(&output).unwrap();
  assert!(report.has_errors());
  assert!(
    report
      .findings()
      .iter()
      .any(|finding| finding.rule() == ValidationRule::MetadataMissing)
  );
}

#[test]
fn plain_output_writes_covering_bbox() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points.parquet");
  let output = temp.path().join("points-passthrough.parquet");
  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let point = wkb_point(1.0, 1.0);
  let batch = arrow_array::RecordBatch::try_new(
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
    .block_on(run(SpatialPipelineOptions::new(
      InputOptions::new(
        input.to_string_lossy(),
        None,
        RowRange::default(),
        None,
        None,
        None,
      ),
      OutputOptions::new(&output, OutputMode::Plain, None, None, 4326, true, true),
    )))
    .unwrap();

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  assert!(batches[0].column_by_name("bbox").is_some());
  let geo: serde_json::Value = serde_json::from_str(kv_map(&output).get("geo").unwrap()).unwrap();
  assert_covering_metadata(&geo);
}

#[test]
fn plain_output_reprojects_selected_rows_and_covering_extent() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points-3857.parquet");
  let output = temp.path().join("points-4326.parquet");
  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let ignored_xy = transform_point_between_epsg(40.0, 30.0, 4326, 3857);
  let selected_xy = transform_point_between_epsg(1.0, 1.0, 4326, 3857);
  let ignored_point = wkb_point(ignored_xy.0, ignored_xy.1);
  let selected_point = wkb_point(selected_xy.0, selected_xy.1);
  let batch = arrow_array::RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(vec!["ignored", "selected", "missing"])),
      Arc::new(BinaryArray::from(vec![
        Some(ignored_point.as_slice()),
        Some(selected_point.as_slice()),
        None,
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

  runtime()
    .block_on(run(SpatialPipelineOptions::new(
      InputOptions::new(
        input.to_string_lossy(),
        None,
        RowRange::new(1, Some(2)),
        None,
        None,
        None,
      ),
      OutputOptions::new(&output, OutputMode::Plain, None, None, 4326, true, true),
    )))
    .unwrap();

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let batch = &batches[0];
  assert_eq!(batch.num_rows(), 2);
  assert_eq!(
    string_value(batch.column_by_name("name").unwrap().as_ref(), 0),
    "selected"
  );
  let geometry = binary_value(batch.column_by_name("geometry").unwrap().as_ref(), 0);
  let (output_x, output_y) = point_xy_from_wkb(&geometry).unwrap();
  assert_close(output_x, 1.0);
  assert_close(output_y, 1.0);

  let bbox = batch
    .column_by_name("bbox")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  assert_close(struct_f64_value(bbox, "xmin", 0), 1.0);
  assert_close(struct_f64_value(bbox, "ymin", 0), 1.0);
  assert_close(struct_f64_value(bbox, "xmax", 0), 1.0);
  assert_close(struct_f64_value(bbox, "ymax", 0), 1.0);
  assert!(bbox.is_null(1));

  let metadata = kv_map(&output);
  let geo: serde_json::Value = serde_json::from_str(metadata.get("geo").unwrap()).unwrap();
  assert_covering_metadata(&geo);
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["code"], 4326);
  let extent = geo["columns"]["geometry"]["bbox"].as_array().unwrap();
  for value in extent {
    assert_close(value.as_f64().unwrap(), 1.0);
  }
  assert!(!metadata.get("geo").unwrap().contains("3857"));
  assert!(
    !metadata
      .get("geo")
      .unwrap()
      .contains(&ignored_xy.0.to_string())
  );
}

#[test]
fn plain_output_rejects_non_wgs84_before_filesystem_mutation() {
  for output_wkid in [3857, 4269] {
    let temp = TempDir::new().unwrap();
    let input = temp.path().join("missing-input.parquet");
    let output = temp.path().join("must-not-exist.parquet");
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
      runtime().block_on(run(SpatialPipelineOptions::new(
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
          OutputMode::Plain,
          None,
          None,
          output_wkid,
          false,
          true,
        ),
      )))
    }));
    assert!(panic.is_err());
    assert!(!output.exists());
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
  }
}

#[test]
fn plain_output_rejects_partition_count() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("input.parquet");
  let output = temp.path().join("output");
  let schema = Arc::new(Schema::new(vec![Field::new(
    "geometry",
    DataType::Binary,
    true,
  )]));
  let point = wkb_point(0.0, 0.0);
  let batch = arrow_array::RecordBatch::try_new(
    schema.clone(),
    vec![Arc::new(BinaryArray::from(vec![Some(point.as_slice())]))],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Point"])],
  );

  let error = runtime()
    .block_on(run(SpatialPipelineOptions::new(
      InputOptions::new(
        input.to_string_lossy(),
        None,
        RowRange::default(),
        None,
        None,
        None,
      ),
      OutputOptions::new(&output, OutputMode::Plain, Some(2), None, 4326, false, true),
    )))
    .unwrap_err();

  assert!(
    error
      .to_string()
      .contains("plain GeoParquet output does not support --output-files")
  );
}

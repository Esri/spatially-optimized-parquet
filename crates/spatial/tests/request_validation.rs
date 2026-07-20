mod common;

use std::sync::Arc;

use arrow_array::{BinaryArray, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema};
use spatial::{InputOptions, OutputMode, OutputOptions, Pipeline, SpatialPipelineOptions};
use tempfile::TempDir;
use tokio::runtime::Runtime;

use common::fixture::wkb_point;
use common::parquet::{geoparquet_kv_with_epsg, kv_map, write_parquet};

fn runtime() -> Runtime {
  Runtime::new().unwrap()
}

#[test]
fn optimized_output_rejects_non_wgs84_before_filesystem_mutation() {
  for output_wkid in [3857, 4269] {
    let temp = TempDir::new().unwrap();
    let input = temp.path().join("missing-input.parquet");
    let output = temp.path().join("must-not-exist.parquet");
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
      runtime().block_on(Pipeline::run(SpatialPipelineOptions {
        input: InputOptions {
          location: input.to_string_lossy().into_owned(),
          ..Default::default()
        },
        output: OutputOptions {
          path: output.clone(),
          mode: OutputMode::Optimized,
          output_wkid,
          overwrite: true,
          ..Default::default()
        },
        ..Default::default()
      }))
    }));
    assert!(panic.is_err());
    assert!(!output.exists());
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
  }
}

#[test]
fn output_rejects_explicit_geometry_without_crs_metadata() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points.parquet");
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

  for output_mode in [OutputMode::Plain, OutputMode::Optimized] {
    let output = temp.path().join(format!("{output_mode:?}.parquet"));
    let error = runtime()
      .block_on(Pipeline::run(SpatialPipelineOptions {
        input: InputOptions {
          location: input.to_string_lossy().into_owned(),
          geometry_column: Some("geometry".to_string()),
          ..Default::default()
        },
        output: OutputOptions {
          path: output.clone(),
          mode: output_mode,
          overwrite: true,
          ..Default::default()
        },
        ..Default::default()
      }))
      .unwrap_err();
    assert!(error.to_string().contains("pass --in-sr"), "{error:#}");
  }

  let output = temp.path().join("plain-with-crs.parquet");
  runtime()
    .block_on(Pipeline::run(SpatialPipelineOptions {
      input: InputOptions {
        location: input.to_string_lossy().into_owned(),
        geometry_column: Some("geometry".to_string()),
        input_wkid: Some(3857),
        ..Default::default()
      },
      output: OutputOptions {
        path: output.clone(),
        mode: OutputMode::Plain,
        overwrite: true,
        ..Default::default()
      },
      ..Default::default()
    }))
    .unwrap();
  let geo: serde_json::Value = serde_json::from_str(kv_map(&output).get("geo").unwrap()).unwrap();
  assert_eq!(
    geo["columns"]["geometry"]["crs"]["id"]["code"],
    serde_json::json!(4326)
  );
}

#[test]
fn output_rejects_input_wkid_when_crs_metadata_exists() {
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
    .block_on(Pipeline::run(SpatialPipelineOptions {
      input: InputOptions {
        location: input.to_string_lossy().into_owned(),
        input_wkid: Some(3857),
        ..Default::default()
      },
      output: OutputOptions {
        path: output.clone(),
        mode: OutputMode::Optimized,
        overwrite: true,
        ..Default::default()
      },
      ..Default::default()
    }))
    .unwrap_err();
  assert!(
    error
      .to_string()
      .contains("already has spatial-reference metadata"),
    "{error:#}"
  );
}

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
fn output_rejects_unsupported_crs_before_filesystem_mutation() {
  for mode in [OutputMode::Plain, OutputMode::Optimized] {
    let temp = TempDir::new().unwrap();
    let input = temp.path().join("missing-input.parquet");
    let output = temp.path().join("must-not-exist.parquet");
    let error = runtime()
      .block_on(Pipeline::run(SpatialPipelineOptions {
        input: InputOptions {
          location: input.to_string_lossy().into_owned(),
          ..Default::default()
        },
        output: OutputOptions {
          path: output.clone(),
          mode,
          output_wkid: 4269,
          overwrite: true,
          ..Default::default()
        },
        ..Default::default()
      }))
      .unwrap_err();
    assert!(
      error
        .to_string()
        .contains("unsupported output spatial reference EPSG:4269"),
      "{error:#}"
    );
    assert!(!output.exists());
    assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
  }
}

#[test]
fn web_mercator_output_rejects_out_of_domain_projected_coordinates() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points.parquet");
  let output = temp.path().join("output.parquet");
  let schema = Arc::new(Schema::new(vec![Field::new(
    "geometry",
    DataType::Binary,
    false,
  )]));
  let point = wkb_point(20_037_509.0, 0.0);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![Arc::new(BinaryArray::from(vec![point.as_slice()]))],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv_with_epsg("geometry", &["Point"], 3857)],
  );

  let error = runtime()
    .block_on(Pipeline::run(SpatialPipelineOptions {
      input: InputOptions {
        location: input.to_string_lossy().into_owned(),
        ..Default::default()
      },
      output: OutputOptions {
        path: output.clone(),
        mode: OutputMode::Plain,
        output_wkid: 3857,
        overwrite: true,
        ..Default::default()
      },
      ..Default::default()
    }))
    .unwrap_err();
  assert!(
    error.to_string().contains("canonical EPSG:3857 bounds"),
    "{error:#}"
  );
}

#[test]
fn web_mercator_output_rejects_out_of_domain_wgs84_latitude() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points.parquet");
  let output = temp.path().join("output.parquet");
  let schema = Arc::new(Schema::new(vec![Field::new(
    "geometry",
    DataType::Binary,
    false,
  )]));
  let point = wkb_point(0.0, 90.0);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![Arc::new(BinaryArray::from(vec![point.as_slice()]))],
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
        ..Default::default()
      },
      output: OutputOptions {
        path: output.clone(),
        mode: OutputMode::Plain,
        output_wkid: 3857,
        overwrite: true,
        ..Default::default()
      },
      ..Default::default()
    }))
    .unwrap_err();
  let message = error.to_string();
  assert!(
    message.contains("reproject geometry") || message.contains("canonical EPSG:3857 bounds"),
    "{error:#}"
  );
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
fn output_applies_geoparquet_default_crs_when_omitted() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points.parquet");
  let output = temp.path().join("points-output.parquet");
  let schema = Arc::new(Schema::new(vec![Field::new(
    "geometry",
    DataType::Binary,
    true,
  )]));
  let point = wkb_point(139.6917, 35.6895);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![Arc::new(BinaryArray::from(vec![Some(point.as_slice())]))],
  )
  .unwrap();
  let geo = serde_json::json!({
    "version": "1.0.0",
    "primary_column": "geometry",
    "columns": {
      "geometry": {
        "encoding": "WKB",
        "geometry_types": ["Point"]
      }
    }
  });
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[parquet::file::metadata::KeyValue::new(
      "geo".to_string(),
      Some(geo.to_string()),
    )],
  );

  runtime()
    .block_on(Pipeline::run(SpatialPipelineOptions {
      input: InputOptions {
        location: input.to_string_lossy().into_owned(),
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

  let output_geo: serde_json::Value =
    serde_json::from_str(kv_map(&output).get("geo").unwrap()).unwrap();
  assert_eq!(
    output_geo["columns"]["geometry"]["crs"]["id"]["code"],
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

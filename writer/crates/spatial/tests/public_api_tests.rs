use spatial::{
  ExecutionOptions, InputOptions, OutputMode, OutputOptions, RowRange, SourceFormat,
  SpatialPipelineOptions, ValidationRule, run, validate,
};
use tempfile::TempDir;
use tokio::runtime::Runtime;

#[path = "../src/test_support/fixtures.rs"]
mod fixtures;
#[path = "../src/test_support/parquet.rs"]
mod parquet_fixture;

use fixtures::{sample_batch_with_geometry, sample_schema_with_geometry, wkb_point};
use parquet_fixture::{geoparquet_kv, write_parquet};

#[test]
fn public_api_runs_typed_request_and_reports_rows_written() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("input.parquet");
  let output = temp.path().join("output.parquet");
  let schema = sample_schema_with_geometry();
  let batch = sample_batch_with_geometry(vec![
    Some(wkb_point(0.0, 0.0)),
    Some(wkb_point(1.0, 1.0)),
    Some(wkb_point(2.0, 2.0)),
  ]);
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Point"])],
  );

  let result = Runtime::new()
    .unwrap()
    .block_on(run(SpatialPipelineOptions::new(
      InputOptions::new(
        input.to_string_lossy(),
        Some(SourceFormat::Parquet),
        RowRange::new(1, Some(1)),
        None,
        None,
        None,
      ),
      OutputOptions::new(&output, OutputMode::Plain, None, None, 4326, false, true),
      ExecutionOptions::new(false, false),
    )))
    .unwrap();

  assert_eq!(result.rows_written(), 1);
  assert!(result.validation_report().is_none());
  assert!(output.exists());

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
fn public_api_validates_optimized_output_and_returns_warnings() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("input.parquet");
  let output = temp.path().join("output.parquet");
  let schema = sample_schema_with_geometry();
  let batch = sample_batch_with_geometry(vec![
    Some(wkb_point(0.0, 0.0)),
    Some(wkb_point(1.0, 1.0)),
    Some(wkb_point(2.0, 2.0)),
  ]);
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Point"])],
  );

  let result = Runtime::new()
    .unwrap()
    .block_on(run(SpatialPipelineOptions::new(
      InputOptions::new(
        input.to_string_lossy(),
        Some(SourceFormat::Parquet),
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
      ExecutionOptions::new(false, false),
    )))
    .unwrap();

  let automatic_report = result.validation_report().expect("automatic validation");
  assert!(!automatic_report.has_errors());
  assert!(
    automatic_report
      .findings()
      .iter()
      .any(|finding| finding.rule() == ValidationRule::WriterMetadata)
  );

  let explicit_report = validate(&output).unwrap();
  assert!(!explicit_report.has_errors());
  assert_eq!(
    explicit_report.warning_count(),
    automatic_report.warning_count()
  );
}

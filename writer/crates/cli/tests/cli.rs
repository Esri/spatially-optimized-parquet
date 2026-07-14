use std::fs::File;
use std::process::Command;
use std::sync::Arc;

use arrow_array::{BinaryArray, RecordBatch};
use arrow_schema::{DataType, Field, Schema};
use parquet::arrow::arrow_writer::ArrowWriter;
use tempfile::TempDir;

fn write_point_input(path: &std::path::Path) {
  let schema = Arc::new(Schema::new(vec![Field::new(
    "geometry",
    DataType::Binary,
    true,
  )]));
  let point_a = point_wkb(0.0, 0.0);
  let point_b = point_wkb(1.0, 1.0);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![Arc::new(BinaryArray::from(vec![
      Some(point_a.as_slice()),
      Some(point_b.as_slice()),
    ]))],
  )
  .unwrap();
  let mut writer = ArrowWriter::try_new(File::create(path).unwrap(), schema, None).unwrap();
  writer.write(&batch).unwrap();
  writer.close().unwrap();
}

fn point_wkb(x: f64, y: f64) -> Vec<u8> {
  let mut bytes = Vec::with_capacity(21);
  bytes.push(1);
  bytes.extend(1_u32.to_le_bytes());
  bytes.extend(x.to_le_bytes());
  bytes.extend(y.to_le_bytes());
  bytes
}

#[test]
fn write_subcommand_prints_automatic_validation_report() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("input.parquet");
  let output = temp.path().join("output.parquet");
  write_point_input(&input);

  let result = Command::new(env!("CARGO_BIN_EXE_parquet-opt"))
    .args([
      "write",
      "--input",
      input.to_str().unwrap(),
      "--output",
      output.to_str().unwrap(),
      "--geometry-column",
      "geometry",
      "--in-sr",
      "4326",
      "--overwrite",
    ])
    .output()
    .unwrap();

  assert!(
    result.status.success(),
    "{}",
    String::from_utf8_lossy(&result.stderr)
  );
  let stdout = String::from_utf8_lossy(&result.stdout);
  assert!(stdout.contains("wrote 2 rows"));
  assert!(stdout.contains("valid:"));
  assert!(stdout.contains("warning SOP-META-006"));
  assert!(output.exists());
}

#[test]
fn validate_subcommand_exits_zero_for_warnings() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("input.parquet");
  let output = temp.path().join("output.parquet");
  write_point_input(&input);
  let write = Command::new(env!("CARGO_BIN_EXE_parquet-opt"))
    .args([
      "write",
      "--input",
      input.to_str().unwrap(),
      "--output",
      output.to_str().unwrap(),
      "--geometry-column",
      "geometry",
      "--in-sr",
      "4326",
      "--overwrite",
    ])
    .output()
    .unwrap();
  assert!(write.status.success());

  let result = Command::new(env!("CARGO_BIN_EXE_parquet-opt"))
    .args(["validate", output.to_str().unwrap()])
    .output()
    .unwrap();

  assert!(result.status.success());
  let stdout = String::from_utf8_lossy(&result.stdout);
  assert!(stdout.contains("0 errors"));
  assert!(stdout.contains("warning SOP-META-006"));
}

#[test]
fn validate_subcommand_exits_nonzero_for_errors_and_preserves_output() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("input.parquet");
  let output = temp.path().join("plain.parquet");
  write_point_input(&input);
  let write = Command::new(env!("CARGO_BIN_EXE_parquet-opt"))
    .args([
      "write",
      "--input",
      input.to_str().unwrap(),
      "--output",
      output.to_str().unwrap(),
      "--geometry-column",
      "geometry",
      "--in-sr",
      "4326",
      "--no-optimization",
      "--overwrite",
    ])
    .output()
    .unwrap();
  assert!(write.status.success());
  assert!(!String::from_utf8_lossy(&write.stdout).contains("valid:"));

  let result = Command::new(env!("CARGO_BIN_EXE_parquet-opt"))
    .args(["validate", output.to_str().unwrap()])
    .output()
    .unwrap();

  assert!(!result.status.success());
  let stderr = String::from_utf8_lossy(&result.stderr);
  assert!(stderr.contains("invalid:"));
  assert!(stderr.contains("SOP-META-001"));
  assert!(output.exists());
}

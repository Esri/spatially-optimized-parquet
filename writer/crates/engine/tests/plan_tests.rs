use std::path::PathBuf;

use tempfile::TempDir;

use engine::plan::{output_paths, target_rows_per_file, validate_output};

#[test]
fn validate_output_directory_requires_parts() {
  let temp = TempDir::new().unwrap();
  let out_dir = temp.path().join("out_dir");
  let err = validate_output(&out_dir, None, false).unwrap_err();
  assert!(err.to_string().contains("output-files"));
}

#[test]
fn validate_output_file_requires_single_part() {
  let temp = TempDir::new().unwrap();
  let out = temp.path().join("out.parquet");
  let err = validate_output(&out, Some(2), false).unwrap_err();
  assert!(err.to_string().contains("output-files"));
}

#[test]
fn validate_output_directory_rejects_zero_parts() {
  let temp = TempDir::new().unwrap();
  let out = temp.path().join("out_dir");
  let err = validate_output(&out, Some(0), false).unwrap_err();
  assert!(err.to_string().contains("output-files"));
}

#[test]
fn validate_output_existing_path_requires_overwrite() {
  let temp = TempDir::new().unwrap();
  let out = temp.path().join("out.parquet");
  std::fs::write(&out, "data").unwrap();
  let err = validate_output(&out, None, false).unwrap_err();
  assert!(err.to_string().contains("output path already exists"));
  assert!(err.to_string().contains("--overwrite"));
}

#[test]
fn validate_output_overwrite_recreates_directory() {
  let temp = TempDir::new().unwrap();
  let out_dir = temp.path().join("out");
  std::fs::create_dir_all(&out_dir).unwrap();
  std::fs::write(out_dir.join("stale.parquet"), "stale").unwrap();

  let plan = validate_output(&out_dir, Some(1), true).unwrap();

  assert!(plan.is_directory);
  assert!(out_dir.exists());
  assert!(out_dir.is_dir());
  assert!(!out_dir.join("stale.parquet").exists());
}

#[test]
fn output_paths_directory_layout() {
  let temp = TempDir::new().unwrap();
  let plan = validate_output(&temp.path().join("out"), Some(3), false).unwrap();
  let paths = output_paths(&plan).unwrap();
  let expected: Vec<PathBuf> = vec![
    temp.path().join("out/part-00000.parquet"),
    temp.path().join("out/part-00001.parquet"),
    temp.path().join("out/part-00002.parquet"),
  ];
  assert_eq!(paths, expected);
}

#[test]
fn target_rows_per_file_bounds() {
  assert_eq!(target_rows_per_file(1, 100), 0);
  assert_eq!(target_rows_per_file(2, 0), 0);
  assert_eq!(target_rows_per_file(2, 1), 1);
  assert_eq!(target_rows_per_file(4, 10), 2);
}

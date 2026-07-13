use std::path::PathBuf;

use tempfile::TempDir;

use engine::output_layout::{resolve_output_layout, resolved_output_paths};

#[test]
fn resolve_output_layout_directory_requires_parts() {
  let temp = TempDir::new().unwrap();
  let out_dir = temp.path().join("out_dir");
  let err = resolve_output_layout(&out_dir, None, false).unwrap_err();
  assert!(err.to_string().contains("output-files"));
}

#[test]
fn resolve_output_layout_file_requires_single_part() {
  let temp = TempDir::new().unwrap();
  let out = temp.path().join("out.parquet");
  let err = resolve_output_layout(&out, Some(2), false).unwrap_err();
  assert!(err.to_string().contains("output-files"));
}

#[test]
fn resolve_output_layout_directory_rejects_zero_parts() {
  let temp = TempDir::new().unwrap();
  let out = temp.path().join("out_dir");
  let err = resolve_output_layout(&out, Some(0), false).unwrap_err();
  assert!(err.to_string().contains("output-files"));
}

#[test]
fn resolve_output_layout_existing_path_requires_overwrite() {
  let temp = TempDir::new().unwrap();
  let out = temp.path().join("out.parquet");
  std::fs::write(&out, "data").unwrap();
  let err = resolve_output_layout(&out, None, false).unwrap_err();
  assert!(err.to_string().contains("output path already exists"));
  assert!(err.to_string().contains("--overwrite"));
}

#[test]
fn resolve_output_layout_overwrite_recreates_directory() {
  let temp = TempDir::new().unwrap();
  let out_dir = temp.path().join("out");
  std::fs::create_dir_all(&out_dir).unwrap();
  std::fs::write(out_dir.join("stale.parquet"), "stale").unwrap();

  let layout = resolve_output_layout(&out_dir, Some(1), true).unwrap();

  assert!(layout.is_directory);
  assert!(out_dir.exists());
  assert!(out_dir.is_dir());
  assert!(!out_dir.join("stale.parquet").exists());
}

#[test]
fn resolved_output_paths_directory_layout() {
  let temp = TempDir::new().unwrap();
  let layout = resolve_output_layout(&temp.path().join("out"), Some(3), false).unwrap();
  let paths = resolved_output_paths(&layout).unwrap();
  let expected: Vec<PathBuf> = vec![
    temp.path().join("out/part-00000.parquet"),
    temp.path().join("out/part-00001.parquet"),
    temp.path().join("out/part-00002.parquet"),
  ];
  assert_eq!(paths, expected);
}

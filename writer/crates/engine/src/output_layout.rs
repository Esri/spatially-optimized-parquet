//! Resolves a user-facing output destination into a safe, concrete Parquet file layout.
//!
//! A path with an extension represents one file. A path without an extension represents a
//! directory and therefore requires an explicit part count. [`OutputLayout::new`] enforces those
//! rules, protects existing output unless overwrite was requested, removes only compatible
//! directory destinations, and creates the required parent directories.
//!
//! The resulting [`OutputLayout`] acts as the invariant-bearing boundary for later writers.
//! Downstream code can generate deterministic part names and distribute rows without
//! repeating path validation or filesystem mutation policy.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

#[derive(Debug)]
/// Describes the resolved output destination and number of Parquet parts.
pub struct OutputLayout {
  is_directory: bool,
  path: PathBuf,
  parts: usize,
}

#[derive(Debug, thiserror::Error)]
enum OutputLayoutError {
  /// Indicates that replacement was not authorized for an existing destination.
  #[error("output path already exists: {0} (pass --overwrite to replace it)")]
  Exists(PathBuf),
  /// Indicates that directory output omitted its required file count.
  #[error("output path requires --output-files when output is a directory")]
  FilesRequired,
  /// Indicates that a file output requested more than one part.
  #[error("output path is a file so --output-files must be 1")]
  FilesMustBeOne,
  /// Indicates that the requested output file count was zero.
  #[error("--output-files must be >= 1")]
  FilesInvalid,
}

impl OutputLayout {
  /// Resolve the output layout and prepare its parent directory.
  ///
  /// Existing compatible destinations are removed only when `overwrite` is true.
  pub fn new(output: &Path, output_files: Option<usize>, overwrite: bool) -> Result<Self> {
    let has_extension = output.extension().is_some();
    let is_directory = !has_extension;
    let parts = if is_directory {
      let parts = output_files.ok_or(OutputLayoutError::FilesRequired)?;
      if parts == 0 {
        return Err(OutputLayoutError::FilesInvalid.into());
      }
      parts
    } else {
      let parts = output_files.unwrap_or(1);
      if parts != 1 {
        return Err(OutputLayoutError::FilesMustBeOne.into());
      }
      parts
    };

    if output.exists() {
      if !overwrite {
        return Err(OutputLayoutError::Exists(output.to_path_buf()).into());
      }
      if output.is_dir() && !is_directory {
        return Err(OutputLayoutError::Exists(output.to_path_buf()).into());
      }
      if output.is_file() && is_directory {
        return Err(OutputLayoutError::Exists(output.to_path_buf()).into());
      }
      if output.is_dir() && is_directory {
        fs::remove_dir_all(output)?;
      }
    }

    if is_directory {
      fs::create_dir_all(output)?;
    } else if let Some(parent) = output.parent() {
      fs::create_dir_all(parent)?;
    }

    Ok(Self {
      is_directory,
      path: output.to_path_buf(),
      parts,
    })
  }

  /// Return the output file or directory selected by the caller.
  pub fn path(&self) -> &Path {
    &self.path
  }

  /// Return the exact number of output files to create.
  pub fn part_count(&self) -> usize {
    self.parts
  }

  /// Resolve the validated layout into deterministic output file paths.
  pub fn paths(&self) -> Result<Vec<PathBuf>> {
    if self.is_directory {
      let mut paths = Vec::new();
      for part_index in 0..self.parts {
        let filename = format!("part-{part_index:05}.parquet");
        paths.push(self.path.join(filename));
      }
      Ok(paths)
    } else {
      Ok(vec![self.path.clone()])
    }
  }
}

#[cfg(test)]
mod tests {
  use std::path::PathBuf;

  use tempfile::TempDir;

  use super::OutputLayout;

  #[test]
  fn directory_layout_requires_parts() {
    let temp = TempDir::new().unwrap();
    let out_dir = temp.path().join("out_dir");
    let error = OutputLayout::new(&out_dir, None, false).unwrap_err();
    assert!(error.to_string().contains("output-files"));
  }

  #[test]
  fn file_layout_requires_single_part() {
    let temp = TempDir::new().unwrap();
    let output = temp.path().join("out.parquet");
    let error = OutputLayout::new(&output, Some(2), false).unwrap_err();
    assert!(error.to_string().contains("output-files"));
  }

  #[test]
  fn directory_layout_rejects_zero_parts() {
    let temp = TempDir::new().unwrap();
    let out_dir = temp.path().join("out_dir");
    let error = OutputLayout::new(&out_dir, Some(0), false).unwrap_err();
    assert!(error.to_string().contains("output-files"));
  }

  #[test]
  fn existing_path_requires_overwrite() {
    let temp = TempDir::new().unwrap();
    let output = temp.path().join("out.parquet");
    std::fs::write(&output, "data").unwrap();
    let error = OutputLayout::new(&output, None, false).unwrap_err();
    assert!(error.to_string().contains("output path already exists"));
    assert!(error.to_string().contains("--overwrite"));
  }

  #[test]
  fn overwrite_recreates_directory() {
    let temp = TempDir::new().unwrap();
    let out_dir = temp.path().join("out");
    std::fs::create_dir_all(&out_dir).unwrap();
    std::fs::write(out_dir.join("stale.parquet"), "stale").unwrap();

    let layout = OutputLayout::new(&out_dir, Some(1), true).unwrap();

    assert!(layout.is_directory);
    assert_eq!(layout.path(), out_dir);
    assert_eq!(layout.part_count(), 1);
    assert!(out_dir.exists());
    assert!(out_dir.is_dir());
    assert!(!out_dir.join("stale.parquet").exists());
  }

  #[test]
  fn directory_paths_use_deterministic_part_names() {
    let temp = TempDir::new().unwrap();
    let layout = OutputLayout::new(&temp.path().join("out"), Some(3), false).unwrap();
    let paths = layout.paths().unwrap();
    let expected: Vec<PathBuf> = vec![
      temp.path().join("out/part-00000.parquet"),
      temp.path().join("out/part-00001.parquet"),
      temp.path().join("out/part-00002.parquet"),
    ];
    assert_eq!(paths, expected);
  }
}

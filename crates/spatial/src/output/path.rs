// Copyright 2026 Esri
//
// Licensed under the Apache License Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Resolves a user-facing output destination into safe, concrete Parquet file paths.
//!
//! A path with an extension represents one file. A path without an extension represents a
//! directory and therefore requires an explicit part count. [`OutputPath::new`] enforces those
//! rules, protects existing output unless overwrite was requested, removes only compatible
//! directory destinations, and creates the required parent directories.
//!
//! The resulting [`OutputPath`] acts as the invariant-bearing boundary for later writers.
//! Downstream code can generate deterministic part names and distribute rows without repeating
//! path validation or filesystem mutation policy.

use std::fs;
use std::path::{Path, PathBuf};

use super::OutputError;

#[derive(Debug)]
/// Describes the resolved output destination and number of Parquet parts.
pub(crate) struct OutputPath {
  is_directory: bool,
  path: PathBuf,
  parts: usize,
}

#[derive(Debug, thiserror::Error)]
enum OutputPathError {
  /// Indicates that replacement was not authorized for an existing destination.
  #[error("output path already exists: {0} (pass --overwrite to replace it)")]
  Exists(PathBuf),
  /// Indicates that directory output omitted its required file count.
  #[error("output path requires --partitions when output is a directory")]
  FilesRequired,
  /// Indicates that a file output requested more than one part.
  #[error("output path is a file so --partitions must be 1")]
  FilesMustBeOne,
  /// Indicates that the requested output file count was zero.
  #[error("--partitions must be >= 1")]
  FilesInvalid,
}

impl OutputPath {
  /// Resolve the output path and prepare its parent directory.
  ///
  /// Existing compatible destinations are removed only when `overwrite` is true.
  pub(crate) fn new(
    output: &Path,
    output_files: Option<usize>,
    overwrite: bool,
  ) -> Result<Self, OutputError> {
    let has_extension = output.extension().is_some();
    let is_directory = !has_extension;
    let parts = if is_directory {
      let parts = output_files
        .ok_or_else(|| OutputError::Path(OutputPathError::FilesRequired.to_string()))?;
      if parts == 0 {
        return Err(OutputError::Path(OutputPathError::FilesInvalid.to_string()));
      }
      parts
    } else {
      let parts = output_files.unwrap_or(1);
      if parts != 1 {
        return Err(OutputError::Path(
          OutputPathError::FilesMustBeOne.to_string(),
        ));
      }
      parts
    };

    if output.exists() {
      if !overwrite {
        return Err(OutputError::Path(
          OutputPathError::Exists(output.to_path_buf()).to_string(),
        ));
      }
      if output.is_dir() && !is_directory {
        return Err(OutputError::Path(
          OutputPathError::Exists(output.to_path_buf()).to_string(),
        ));
      }
      if output.is_file() && is_directory {
        return Err(OutputError::Path(
          OutputPathError::Exists(output.to_path_buf()).to_string(),
        ));
      }
      if output.is_dir() && is_directory {
        fs::remove_dir_all(output).map_err(|source| OutputError::Io {
          operation: "remove output directory",
          path: output.to_path_buf(),
          source,
        })?;
      }
    }

    if is_directory {
      fs::create_dir_all(output).map_err(|source| OutputError::Io {
        operation: "create output directory",
        path: output.to_path_buf(),
        source,
      })?;
    } else if let Some(parent) = output.parent() {
      fs::create_dir_all(parent).map_err(|source| OutputError::Io {
        operation: "create output parent directory",
        path: parent.to_path_buf(),
        source,
      })?;
    }

    Ok(Self {
      is_directory,
      path: output.to_path_buf(),
      parts,
    })
  }

  /// Return the output file or directory selected by the caller.
  pub(crate) fn path(&self) -> &Path {
    &self.path
  }

  /// Return the exact number of output files to create.
  pub(crate) fn part_count(&self) -> usize {
    self.parts
  }

  /// Resolve the validated output path into deterministic output file paths.
  pub(crate) fn paths(&self) -> Result<Vec<PathBuf>, OutputError> {
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
  use tempfile::TempDir;

  use super::OutputPath;

  #[test]
  fn existing_path_requires_overwrite() {
    let temp = TempDir::new().unwrap();
    let output = temp.path().join("out.parquet");
    std::fs::write(&output, "data").unwrap();
    let error = OutputPath::new(&output, None, false).unwrap_err();
    assert!(error.to_string().contains("output path already exists"));
    assert!(error.to_string().contains("--overwrite"));
  }

  #[test]
  fn overwrite_recreates_directory() {
    let temp = TempDir::new().unwrap();
    let out_dir = temp.path().join("out");
    std::fs::create_dir_all(&out_dir).unwrap();
    std::fs::write(out_dir.join("stale.parquet"), "stale").unwrap();

    let output_path = OutputPath::new(&out_dir, Some(1), true).unwrap();

    assert!(output_path.is_directory);
    assert_eq!(output_path.path(), out_dir);
    assert_eq!(output_path.part_count(), 1);
    assert!(out_dir.exists());
    assert!(out_dir.is_dir());
    assert!(!out_dir.join("stale.parquet").exists());
  }
}

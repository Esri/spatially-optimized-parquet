//! Resolves a user-facing output destination into a safe, concrete Parquet file layout.
//!
//! A path with an extension represents one file. A path without an extension represents a
//! directory and therefore requires an explicit part count. [`validate_output`] enforces
//! those rules, protects existing output unless overwrite was requested, removes only
//! compatible directory destinations, and creates the required parent directories.
//!
//! The resulting [`OutputPlan`] acts as the invariant-bearing boundary for later writers.
//! Downstream code can generate deterministic part names and distribute rows without
//! repeating path validation or filesystem mutation policy.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

#[derive(Debug)]
/// Describes the validated output destination and number of Parquet parts.
pub struct OutputPlan {
  /// Indicates whether `path` names a directory containing generated part files.
  pub is_directory: bool,
  /// Stores the output file or directory selected by the caller.
  pub path: PathBuf,
  /// Stores the exact number of output files to create.
  pub parts: usize,
}

#[derive(Debug, thiserror::Error)]
/// Reports invalid or unsafe output-layout requests.
pub enum PlanError {
  /// Indicates that replacement was not authorized for an existing destination.
  #[error("output path already exists: {0} (pass --overwrite to replace it)")]
  OutputExists(PathBuf),
  /// Indicates that directory output omitted its required file count.
  #[error("output path requires --output-files when output is a directory")]
  OutputFilesRequired,
  /// Indicates that a file output requested more than one part.
  #[error("output path is a file so --output-files must be 1")]
  OutputFilesMustBeOne,
  /// Indicates that the requested output file count was zero.
  #[error("--output-files must be >= 1")]
  OutputFilesInvalid,
}

/// Validate the output layout and prepare its parent directory.
///
/// Existing compatible destinations are removed only when `overwrite` is true.
pub fn validate_output(
  output: &Path,
  output_files: Option<usize>,
  overwrite: bool,
) -> Result<OutputPlan> {
  let has_extension = output.extension().is_some();
  let is_directory = !has_extension;
  let parts = if is_directory {
    let parts = output_files.ok_or(PlanError::OutputFilesRequired)?;
    if parts == 0 {
      return Err(PlanError::OutputFilesInvalid.into());
    }
    parts
  } else {
    let parts = output_files.unwrap_or(1);
    if parts != 1 {
      return Err(PlanError::OutputFilesMustBeOne.into());
    }
    parts
  };

  if output.exists() {
    if !overwrite {
      return Err(PlanError::OutputExists(output.to_path_buf()).into());
    }
    if output.is_dir() && !is_directory {
      return Err(PlanError::OutputExists(output.to_path_buf()).into());
    }
    if output.is_file() && is_directory {
      return Err(PlanError::OutputExists(output.to_path_buf()).into());
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

  Ok(OutputPlan {
    is_directory,
    path: output.to_path_buf(),
    parts,
  })
}

/// Resolve a validated plan into deterministic output file paths.
pub fn output_paths(plan: &OutputPlan) -> Result<Vec<PathBuf>> {
  if plan.is_directory {
    let mut paths = Vec::new();
    for idx in 0..plan.parts {
      let filename = format!("part-{idx:05}.parquet");
      paths.push(plan.path.join(filename));
    }
    Ok(paths)
  } else {
    Ok(vec![plan.path.clone()])
  }
}

/// Calculate the approximate row threshold used to advance sequential writers.
///
/// Returns zero for a single output or empty input because no rollover is needed.
pub fn target_rows_per_file(parts: usize, total_rows: u64) -> u64 {
  if parts <= 1 {
    return 0;
  }
  if total_rows == 0 {
    return 0;
  }
  (total_rows / parts as u64).max(1)
}

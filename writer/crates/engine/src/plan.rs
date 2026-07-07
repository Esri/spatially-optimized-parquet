use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

#[derive(Debug)]
pub struct OutputPlan {
  pub is_directory: bool,
  pub path: PathBuf,
  pub parts: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum PlanError {
  #[error("output path already exists: {0} (pass --overwrite to replace it)")]
  OutputExists(PathBuf),
  #[error("output path requires --output-files when output is a directory")]
  OutputFilesRequired,
  #[error("output path is a file so --output-files must be 1")]
  OutputFilesMustBeOne,
  #[error("--output-files must be >= 1")]
  OutputFilesInvalid,
}

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

pub fn target_rows_per_file(parts: usize, total_rows: u64) -> u64 {
  if parts <= 1 {
    return 0;
  }
  if total_rows == 0 {
    return 0;
  }
  (total_rows / parts as u64).max(1)
}

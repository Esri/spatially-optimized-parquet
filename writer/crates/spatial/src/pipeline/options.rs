//! Defines the stable request vocabulary accepted by the spatial pipeline.

use std::path::PathBuf;

use crate::input::{RowRange, SourceFormat};
use crate::output::OutputMode;

/// Configures one input source and selected row range.
#[derive(Debug, Clone)]
pub struct InputOptions {
  pub(super) location: String,
  pub(super) format: Option<SourceFormat>,
  pub(super) row_range: RowRange,
  pub(super) layer: Option<String>,
  pub(super) geometry_column: Option<String>,
  pub(super) input_wkid: Option<u32>,
}

impl InputOptions {
  /// Construct input options from source, selection, and geometry overrides.
  pub fn new(
    location: impl Into<String>,
    format: Option<SourceFormat>,
    row_range: RowRange,
    layer: Option<String>,
    geometry_column: Option<String>,
    input_wkid: Option<u32>,
  ) -> Self {
    Self {
      location: location.into(),
      format,
      row_range,
      layer,
      geometry_column,
      input_wkid,
    }
  }
}

/// Configures one durable GeoParquet output.
#[derive(Debug, Clone)]
pub struct OutputOptions {
  pub(super) path: PathBuf,
  pub(super) mode: OutputMode,
  pub(super) file_count: Option<usize>,
  pub(super) compression: Option<String>,
  pub(super) output_wkid: u32,
  pub(super) covering: bool,
  pub(super) overwrite: bool,
}

impl OutputOptions {
  /// Construct output options from path, product, storage, and CRS policy.
  pub fn new(
    path: impl Into<PathBuf>,
    mode: OutputMode,
    file_count: Option<usize>,
    compression: Option<String>,
    output_wkid: u32,
    covering: bool,
    overwrite: bool,
  ) -> Self {
    Self {
      path: path.into(),
      mode,
      file_count,
      compression,
      output_wkid,
      covering,
      overwrite,
    }
  }
}

/// Configures diagnostics and interactive execution reporting.
#[derive(Debug, Clone, Copy)]
pub struct ExecutionOptions {
  pub(super) progress: bool,
  pub(super) explain: bool,
}

impl ExecutionOptions {
  /// Construct execution options from progress and diagnostic controls.
  pub fn new(progress: bool, explain: bool) -> Self {
    Self { progress, explain }
  }
}

/// Configures one complete spatial pipeline.
#[derive(Debug, Clone)]
pub struct SpatialPipelineOptions {
  pub(super) input: InputOptions,
  pub(super) output: OutputOptions,
  pub(super) execution: ExecutionOptions,
}

impl SpatialPipelineOptions {
  /// Construct one spatial request from typed input, output, and execution sections.
  pub fn new(input: InputOptions, output: OutputOptions, execution: ExecutionOptions) -> Self {
    Self {
      input,
      output,
      execution,
    }
  }
}

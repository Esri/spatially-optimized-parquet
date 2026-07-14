//! Defines the stable request vocabulary accepted by the spatial pipeline.

use std::path::PathBuf;
use std::sync::Arc;

use crate::input::{RowRange, SourceFormat};
use crate::output::OutputMode;

use super::{SharedWriteReporter, WriteReporter};

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

/// Configures one complete spatial pipeline.
#[derive(Clone)]
pub struct SpatialPipelineOptions {
  pub(super) input: InputOptions,
  pub(super) output: OutputOptions,
  pub(super) write_reporter: Option<SharedWriteReporter>,
}

impl SpatialPipelineOptions {
  /// Construct one spatial request from typed input and output sections.
  pub fn new(input: InputOptions, output: OutputOptions) -> Self {
    Self {
      input,
      output,
      write_reporter: None,
    }
  }

  /// Attach an optional callback for cumulative output row counts.
  pub fn with_write_reporter(mut self, reporter: impl WriteReporter + 'static) -> Self {
    self.write_reporter = Some(Arc::new(reporter));
    self
  }
}

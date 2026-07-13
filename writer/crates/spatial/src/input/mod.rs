//! Defines the format-neutral input boundary used by every spatial job stage.
//!
//! An [`InputProvider`] performs ordered format detection and opens a recognized location.
//! The resulting [`InputSource`] exposes normalized schema, row count, geometry metadata,
//! direct Arrow batch streaming, and lazy DataFusion construction. Jobs can therefore analyze
//! or execute data without branching on GeoPackage versus Parquet behavior.
//!
//! [`open_input`] treats `Ok(None)` as “not my format” and continues to the next provider.
//! Once a provider recognizes a source, its errors stop detection so corrupt or invalid input
//! does not get disguised as an unsupported format. [`RowRange`] carries one contiguous logical
//! row selection across provider implementations, though each format chooses its own physical
//! mechanism for applying that selection.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use ::parquet::file::metadata::KeyValue;
use anyhow::{Context, Result};
use arrow_array::RecordBatch;
use arrow_schema::SchemaRef;
use engine::{DataFrame, SessionContext};
use futures_util::Stream;
use futures_util::future::BoxFuture;

use crate::geometry::GeometrySpec;
use crate::metadata::source::SourceDatasetMetadata;

pub mod gpkg;
pub mod parquet;

/// Streams fallible Arrow batches without tying callers to a provider implementation.
pub type InputBatchStream = Pin<Box<dyn Stream<Item = Result<RecordBatch>> + Send + 'static>>;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
/// Selects a zero-based contiguous range of source rows.
pub struct RowRange {
  /// Stores the number of leading rows to skip.
  pub start: usize,
  /// Stores the maximum rows to read after `start`, or no limit.
  pub num: Option<usize>,
}

impl RowRange {
  /// Return whether the range selects the complete input.
  pub fn is_full(self) -> bool {
    self.start == 0 && self.num.is_none()
  }

  /// Clamp the requested range to a known source row count.
  pub fn effective_rows(self, total_rows: u64) -> u64 {
    let remaining = total_rows.saturating_sub(self.start as u64);
    self
      .num
      .map(|num| remaining.min(num as u64))
      .unwrap_or(remaining)
  }
}

#[derive(Debug, Clone)]
/// Carries provider-neutral input location and layer selection options.
pub struct InputOpenOptions {
  /// Stores a local path or HTTP URL.
  pub location: String,
  /// Stores the requested vector layer for multi-layer formats.
  pub layer: Option<String>,
}

impl InputOpenOptions {
  /// Build options for a local path without an explicit layer.
  pub fn new(path: PathBuf) -> Self {
    Self {
      location: path.to_string_lossy().into_owned(),
      layer: None,
    }
  }

  /// Return the local path when the location does not use HTTP.
  pub fn local_path(&self) -> Option<&Path> {
    (!is_http_url(&self.location)).then(|| Path::new(&self.location))
  }
}

/// Defines the format-neutral contract consumed by spatial jobs.
pub trait InputSource: Send + Sync {
  /// Return a stable display name for diagnostics.
  fn format_name(&self) -> &'static str;
  /// Return the original source path or URL.
  fn source_location(&self) -> &str;
  /// Load the normalized Arrow schema.
  fn schema(&self) -> Result<SchemaRef>;
  /// Return the source row count discovered from format metadata.
  fn total_rows(&self) -> Result<u64>;
  /// Infer the geometry column and encoding when source metadata permits it.
  fn inferred_geometry_spec(&self) -> Result<Option<GeometrySpec>>;
  /// Return normalized geometry and pass-through file metadata.
  fn source_metadata(&self) -> Result<SourceDatasetMetadata>;
  /// Return file-level metadata suitable for propagation to output.
  fn file_metadata(&self) -> Result<Vec<KeyValue>> {
    Ok(self.source_metadata()?.passthrough_kv)
  }
  /// Stream a selected row range directly as Arrow batches.
  fn read_batches(&self, row_range: RowRange) -> BoxFuture<'_, Result<InputBatchStream>>;
  /// Build a lazy DataFusion DataFrame for a selected row range.
  fn to_dataframe<'a>(
    &'a self,
    ctx: &'a SessionContext,
    row_range: RowRange,
  ) -> BoxFuture<'a, Result<DataFrame>>;
}

/// Detects and opens one storage format without claiming unsupported locations.
pub trait InputProvider: Send + Sync {
  /// Return a stable provider name for error context.
  fn name(&self) -> &'static str;
  /// Open a compatible input, returning `None` when another provider should try.
  fn open<'a>(
    &'a self,
    options: &'a InputOpenOptions,
  ) -> BoxFuture<'a, Result<Option<Arc<dyn InputSource>>>>;
}

/// Open an input with the first provider that recognizes it.
///
/// Provider errors stop detection because they indicate a recognized but invalid source.
pub async fn open_input(
  options: &InputOpenOptions,
  providers: &[Box<dyn InputProvider>],
) -> Result<Arc<dyn InputSource>> {
  if let Some(path) = options.local_path()
    && !path.exists()
  {
    return Err(anyhow::anyhow!(
      "input path does not exist: {}",
      path.display()
    ));
  }

  for provider in providers {
    if let Some(source) = provider
      .open(options)
      .await
      .with_context(|| provider.name().to_string())?
    {
      return Ok(source);
    }
  }

  Err(anyhow::anyhow!(
    "no input provider could open input: {}",
    options.location
  ))
}

/// Return whether a location uses an HTTP or HTTPS scheme.
pub fn is_http_url(value: &str) -> bool {
  value.starts_with("http://") || value.starts_with("https://")
}

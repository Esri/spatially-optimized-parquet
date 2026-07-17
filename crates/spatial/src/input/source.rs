//! Defines the format-neutral source contract consumed by GeoParquet output workflows.
//!
//! Concrete source modules normalize storage-specific metadata and execution behind
//! [`InputSource`]. Callers can therefore select row ranges, inspect geometry metadata,
//! or construct lazy DataFusion plans without branching by format.

use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use arrow_schema::SchemaRef;
use datafusion::dataframe::DataFrame;
use datafusion::execution::context::SessionContext;
use futures_util::future::BoxFuture;

use super::{SourceDatasetMetadata, SourceFormat, gpkg, parquet};
use crate::geometry::GeometryColumn;

/// Selects a zero-based contiguous range of source rows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RowRange {
  /// Defines the number of leading rows to skip.
  start: usize,
  /// Limits rows read after `start`, or leaves them unbounded.
  num: Option<usize>,
}

impl RowRange {
  /// Construct a contiguous row selection from its offset and optional limit.
  pub fn new(start: usize, num: Option<usize>) -> Self {
    Self { start, num }
  }

  /// Return the number of leading source rows to skip.
  pub(crate) fn start(self) -> usize {
    self.start
  }

  /// Return the maximum selected row count, or no limit.
  pub(crate) fn num(self) -> Option<usize> {
    self.num
  }

  /// Return whether the range selects the complete input.
  pub(crate) fn is_full(self) -> bool {
    self.start == 0 && self.num.is_none()
  }

  /// Clamp the requested range to a known source row count.
  pub(crate) fn effective_rows(self, total_rows: u64) -> u64 {
    let remaining = total_rows.saturating_sub(self.start as u64);
    self
      .num
      .map(|num| remaining.min(num as u64))
      .unwrap_or(remaining)
  }
}

/// Carries source location and optional vector-layer selection.
#[derive(Debug, Clone)]
pub(crate) struct InputOpenOptions {
  /// Identifies a local path or HTTP URL.
  location: String,
  /// Identifies the requested vector layer for a multi-layer format.
  layer: Option<String>,
}

impl InputOpenOptions {
  /// Construct source-open options from a location and optional vector layer.
  pub(crate) fn new(location: impl Into<String>, layer: Option<String>) -> Self {
    Self {
      location: location.into(),
      layer,
    }
  }

  /// Return whether the location uses HTTP or HTTPS.
  pub(super) fn is_http(&self) -> bool {
    is_http_location(&self.location)
  }

  /// Return the local path when the location does not use HTTP.
  pub(super) fn local_path(&self) -> Option<&Path> {
    (!self.is_http()).then(|| Path::new(&self.location))
  }

  /// Return the original local path or HTTP URL.
  pub(super) fn location(&self) -> &str {
    &self.location
  }

  /// Return the requested vector layer when one was supplied.
  pub(super) fn layer(&self) -> Option<&str> {
    self.layer.as_deref()
  }
}

pub(super) fn is_http_location(value: &str) -> bool {
  value.starts_with("http://") || value.starts_with("https://")
}

/// Open one source implementation selected by a resolved physical format.
pub(crate) async fn open_input(
  format: SourceFormat,
  options: &InputOpenOptions,
) -> Result<Arc<dyn InputSource>> {
  if let Some(path) = options.local_path()
    && !path.exists()
  {
    return Err(anyhow::anyhow!(
      "input path does not exist: {}",
      path.display()
    ));
  }

  match format {
    SourceFormat::GeoPackage => gpkg::GpkgInputSource::open(options).await,
    SourceFormat::Parquet => parquet::ParquetInputSource::open(options).await,
  }
}

/// Defines the normalized input contract consumed by output layout resolution.
pub(crate) trait InputSource: Send + Sync {
  /// Load the normalized Arrow schema.
  fn schema(&self) -> Result<SchemaRef>;
  /// Return the row count discovered from source metadata.
  fn total_rows(&self) -> Result<u64>;
  /// Infer the geometry column and encoding when metadata permits it.
  fn inferred_geometry_column(&self) -> Result<Option<GeometryColumn>>;
  /// Return normalized geometry and pass-through file metadata.
  fn source_metadata(&self) -> Result<SourceDatasetMetadata>;
  /// Create a lazy DataFusion DataFrame for a selected row range.
  fn to_dataframe<'a>(
    &'a self,
    ctx: &'a SessionContext,
    row_range: RowRange,
  ) -> BoxFuture<'a, Result<DataFrame>>;
}

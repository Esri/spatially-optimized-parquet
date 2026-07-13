//! Defines the format-neutral source contract consumed by GeoParquet output workflows.
//!
//! Concrete source modules normalize storage-specific metadata and execution behind
//! [`InputSource`]. Callers can therefore select row ranges, inspect geometry metadata,
//! stream Arrow batches, or construct lazy DataFusion plans without branching by format.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use ::parquet::file::metadata::KeyValue;
use anyhow::Result;
use arrow_array::RecordBatch;
use arrow_schema::SchemaRef;
use engine::{DataFrame, SessionContext};
use futures_util::Stream;
use futures_util::future::BoxFuture;

use crate::geometry::GeometrySpec;
use crate::geoparquet::metadata::source::SourceDatasetMetadata;

use super::{SourceFormat, gpkg, parquet};

/// Streams fallible Arrow batches without exposing a source implementation.
pub type InputBatchStream = Pin<Box<dyn Stream<Item = Result<RecordBatch>> + Send + 'static>>;

/// Selects a zero-based contiguous range of source rows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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

/// Carries source location and optional vector-layer selection.
#[derive(Debug, Clone)]
pub struct InputOpenOptions {
  /// Stores a local path or HTTP URL.
  pub location: String,
  /// Stores the requested vector layer for a multi-layer format.
  pub layer: Option<String>,
}

impl InputOpenOptions {
  /// Construct options for a local path without an explicit layer.
  pub fn new(path: PathBuf) -> Self {
    Self {
      location: path.to_string_lossy().into_owned(),
      layer: None,
    }
  }

  /// Return whether the location uses HTTP or HTTPS.
  pub fn is_http(&self) -> bool {
    is_http_location(&self.location)
  }

  /// Return the local path when the location does not use HTTP.
  pub fn local_path(&self) -> Option<&Path> {
    (!self.is_http()).then(|| Path::new(&self.location))
  }
}

pub(crate) fn is_http_location(value: &str) -> bool {
  value.starts_with("http://") || value.starts_with("https://")
}

/// Open one source implementation selected by a resolved physical format.
pub async fn open_input(
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
    SourceFormat::GeoPackage => gpkg::open_source(options).await,
    SourceFormat::Parquet => parquet::open_source(options).await,
  }
}

/// Defines the normalized input contract consumed by output layout resolution.
pub trait InputSource: Send + Sync {
  /// Return a stable display name for diagnostics.
  fn format_name(&self) -> &'static str;
  /// Return the original source path or URL.
  fn source_location(&self) -> &str;
  /// Load the normalized Arrow schema.
  fn schema(&self) -> Result<SchemaRef>;
  /// Return the row count discovered from source metadata.
  fn total_rows(&self) -> Result<u64>;
  /// Infer the geometry column and encoding when metadata permits it.
  fn inferred_geometry_spec(&self) -> Result<Option<GeometrySpec>>;
  /// Return normalized geometry and pass-through file metadata.
  fn source_metadata(&self) -> Result<SourceDatasetMetadata>;
  /// Return file-level metadata suitable for propagation to output.
  fn file_metadata(&self) -> Result<Vec<KeyValue>> {
    Ok(self.source_metadata()?.passthrough_kv)
  }
  /// Stream a selected row range directly as Arrow batches.
  fn read_batches(&self, row_range: RowRange) -> BoxFuture<'_, Result<InputBatchStream>>;
  /// Create a lazy DataFusion DataFrame for a selected row range.
  fn to_dataframe<'a>(
    &'a self,
    ctx: &'a SessionContext,
    row_range: RowRange,
  ) -> BoxFuture<'a, Result<DataFrame>>;
}

#[cfg(test)]
mod tests {
  use std::path::Path;

  use super::InputOpenOptions;

  #[test]
  fn input_open_options_classify_http_and_local_locations() {
    let http = InputOpenOptions {
      location: "https://example.com/data.parquet".to_string(),
      layer: None,
    };
    let local = InputOpenOptions {
      location: "data.parquet".to_string(),
      layer: None,
    };

    assert!(http.is_http());
    assert!(http.local_path().is_none());
    assert!(!local.is_http());
    assert_eq!(local.local_path(), Some(Path::new("data.parquet")));
  }
}

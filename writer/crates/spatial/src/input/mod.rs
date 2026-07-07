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

pub type InputBatchStream = Pin<Box<dyn Stream<Item = Result<RecordBatch>> + Send + 'static>>;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RowRange {
  pub start: usize,
  pub num: Option<usize>,
}

impl RowRange {
  pub fn is_full(self) -> bool {
    self.start == 0 && self.num.is_none()
  }

  pub fn effective_rows(self, total_rows: u64) -> u64 {
    let remaining = total_rows.saturating_sub(self.start as u64);
    self
      .num
      .map(|num| remaining.min(num as u64))
      .unwrap_or(remaining)
  }
}

#[derive(Debug, Clone)]
pub struct InputOpenOptions {
  pub location: String,
  pub layer: Option<String>,
}

impl InputOpenOptions {
  pub fn new(path: PathBuf) -> Self {
    Self {
      location: path.to_string_lossy().into_owned(),
      layer: None,
    }
  }

  pub fn local_path(&self) -> Option<&Path> {
    (!is_http_url(&self.location)).then(|| Path::new(&self.location))
  }
}

pub trait InputSource: Send + Sync {
  fn format_name(&self) -> &'static str;
  fn source_location(&self) -> &str;
  fn schema(&self) -> Result<SchemaRef>;
  fn total_rows(&self) -> Result<u64>;
  fn inferred_geometry_spec(&self) -> Result<Option<GeometrySpec>>;
  fn source_metadata(&self) -> Result<SourceDatasetMetadata>;
  fn file_metadata(&self) -> Result<Vec<KeyValue>> {
    Ok(self.source_metadata()?.passthrough_kv)
  }
  fn read_batches(&self, row_range: RowRange) -> BoxFuture<'_, Result<InputBatchStream>>;
  fn to_dataframe<'a>(
    &'a self,
    ctx: &'a SessionContext,
    row_range: RowRange,
  ) -> BoxFuture<'a, Result<DataFrame>>;
}

pub trait InputProvider: Send + Sync {
  fn name(&self) -> &'static str;
  fn open<'a>(
    &'a self,
    options: &'a InputOpenOptions,
  ) -> BoxFuture<'a, Result<Option<Arc<dyn InputSource>>>>;
}

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

pub fn is_http_url(value: &str) -> bool {
  value.starts_with("http://") || value.starts_with("https://")
}

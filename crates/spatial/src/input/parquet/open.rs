//! Opens local Parquet datasets and direct HTTP Parquet objects as input sources.
//!
//! Discovers local files or loads remote footer metadata so later scans can share one
//! footer-backed source description.

use std::sync::Arc;

use anyhow::{Context, Result};
use object_store::ObjectStore;
use object_store::ObjectStoreExt;
use object_store::http::HttpBuilder;
use object_store::path::Path as ObjectPath;
use parquet::arrow::arrow_reader::{ArrowReaderMetadata, ArrowReaderOptions};
use parquet::arrow::async_reader::ParquetObjectReader;
use url::Url;

use crate::input::{InputOpenOptions, InputSource};

use super::source::{ParquetInputLocation, ParquetInputSource};
use super::{DiscoveryMode, ParquetDataset, ParquetDatasetFile};

impl ParquetInputSource {
  /// Open a local Parquet file set or one direct HTTP Parquet object.
  pub(crate) async fn open(options: &InputOpenOptions) -> Result<Arc<dyn InputSource>> {
    if let Some(layer) = options.layer() {
      return Err(anyhow::anyhow!(
        "parquet input does not support --layer (got '{layer}')"
      ));
    }
    if options.is_http() {
      return Self::open_http(options.location()).await;
    }

    let path = options
      .local_path()
      .context("parquet input requires a local path or HTTP URL")?;
    let dataset = ParquetDataset::discover(path, DiscoveryMode::Flat)?.with_context(|| {
      format!(
        "parquet input must be a .parquet file or directory containing .parquet files: {}",
        path.display()
      )
    })?;
    let metadata = dataset
      .files()
      .iter()
      .map(|file| Self::load_arrow_metadata(file))
      .collect::<Result<Vec<_>>>()?;

    Ok(Arc::new(Self::new(
      ParquetInputLocation::Local {
        input_path: options.location().to_string(),
      },
      metadata,
    )))
  }

  /// Open one HTTP Parquet object and load its footer metadata with range requests.
  async fn open_http(location: &str) -> Result<Arc<dyn InputSource>> {
    if !location
      .split('?')
      .next()
      .is_some_and(|path| path.ends_with(".parquet"))
    {
      return Err(anyhow::anyhow!(
        "HTTP input must point directly to a .parquet file: {location}"
      ));
    }
    let parsed = Url::parse(location).with_context(|| format!("parse input URL: {location}"))?;
    let store_url = Url::parse(&format!(
      "{}://{}",
      parsed.scheme(),
      parsed
        .host_str()
        .context("HTTP input URL must include a host")?
    ))?;
    let object_path = ObjectPath::parse(parsed.path().trim_start_matches('/'))?;
    let store =
      Arc::new(HttpBuilder::new().with_url(store_url.as_str()).build()?) as Arc<dyn ObjectStore>;
    let object_meta = store
      .head(&object_path)
      .await
      .with_context(|| format!("read HTTP parquet metadata: {location}"))?;
    let mut reader = ParquetObjectReader::new(Arc::clone(&store), object_path.clone())
      .with_file_size(object_meta.size);
    let metadata = ArrowReaderMetadata::load_async(&mut reader, ArrowReaderOptions::new())
      .await
      .with_context(|| format!("read parquet footer: {location}"))?;

    Ok(Arc::new(Self::new(
      ParquetInputLocation::Http {
        input_url: location.to_string(),
        store_url,
        store,
      },
      vec![metadata],
    )))
  }

  /// Load Arrow and Parquet metadata from one local file footer.
  fn load_arrow_metadata(file: &ParquetDatasetFile) -> Result<ArrowReaderMetadata> {
    file
      .load_metadata()
      .with_context(|| format!("read arrow metadata: {}", file.path.display()))
  }
}

#[cfg(test)]
mod tests {
  use std::fs;

  use tempfile::TempDir;

  use super::{DiscoveryMode, ParquetDataset, ParquetInputSource};

  #[test]
  fn parquet_discovery_sorts_files_and_ignores_other_extensions() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("b.parquet"), []).unwrap();
    fs::write(temp.path().join("a.parquet"), []).unwrap();
    fs::write(temp.path().join("notes.txt"), []).unwrap();

    let dataset = ParquetDataset::discover(temp.path(), DiscoveryMode::Flat)
      .unwrap()
      .unwrap();

    assert_eq!(
      dataset
        .files()
        .iter()
        .map(|file| file.path.clone())
        .collect::<Vec<_>>(),
      vec![temp.path().join("a.parquet"), temp.path().join("b.parquet")]
    );
  }

  #[test]
  fn parquet_footer_loading_reports_the_file_context() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("broken.parquet");
    fs::write(&path, b"not parquet").unwrap();

    let dataset = ParquetDataset::discover(&path, DiscoveryMode::Flat)
      .unwrap()
      .unwrap();
    let error = ParquetInputSource::load_arrow_metadata(&dataset.files()[0]).unwrap_err();

    assert!(error.to_string().contains("read arrow metadata"));
  }
}

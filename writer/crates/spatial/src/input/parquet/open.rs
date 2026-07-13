use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use object_store::ObjectStore;
use object_store::http::HttpBuilder;
use object_store::path::Path as ObjectPath;
use parquet::arrow::arrow_reader::{ArrowReaderMetadata, ArrowReaderOptions};
use parquet::arrow::async_reader::ParquetObjectReader;
use url::Url;

use crate::input::{InputOpenOptions, InputSource};

use super::source::{ParquetInputLocation, ParquetInputSource};

/// Open a local Parquet file set or one direct HTTP Parquet object.
pub async fn open_source(options: &InputOpenOptions) -> Result<Arc<dyn InputSource>> {
  if let Some(layer) = options.layer.as_deref() {
    return Err(anyhow::anyhow!(
      "parquet input does not support --layer (got '{layer}')"
    ));
  }
  if options.is_http() {
    return open_http_parquet(&options.location).await;
  }

  let path = options
    .local_path()
    .context("parquet input requires a local path or HTTP URL")?;
  let files = discover_parquet_files(path)?.with_context(|| {
    format!(
      "parquet input must be a .parquet file or directory containing .parquet files: {}",
      path.display()
    )
  })?;
  let metadata = files
    .iter()
    .map(|file| load_arrow_metadata(file))
    .collect::<Result<Vec<_>>>()?;

  Ok(Arc::new(ParquetInputSource::new(
    ParquetInputLocation::Local {
      input_path: options.location.clone(),
    },
    options.location.clone(),
    metadata,
  )))
}

/// Open one HTTP Parquet object and load its footer metadata with range requests.
async fn open_http_parquet(location: &str) -> Result<Arc<dyn InputSource>> {
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

  Ok(Arc::new(ParquetInputSource::new(
    ParquetInputLocation::Http {
      input_url: location.to_string(),
      store_url,
      store,
    },
    location.to_string(),
    vec![metadata],
  )))
}

/// Discover a single Parquet file or a sorted directory of Parquet files.
///
/// Returns `None` for unsupported paths so another input provider can attempt them.
fn discover_parquet_files(input: &Path) -> Result<Option<Vec<PathBuf>>> {
  if input.is_file() {
    return Ok(
      input
        .extension()
        .is_some_and(|ext| ext == "parquet")
        .then(|| vec![input.to_path_buf()]),
    );
  }

  if input.is_dir() {
    let mut files = Vec::new();
    for entry in fs::read_dir(input)? {
      let entry = entry?;
      let path = entry.path();
      if path.is_file() && path.extension().is_some_and(|ext| ext == "parquet") {
        files.push(path);
      }
    }
    if files.is_empty() {
      return Err(anyhow::anyhow!(
        "no parquet files found at: {}",
        input.display()
      ));
    }
    files.sort();
    return Ok(Some(files));
  }

  Ok(None)
}

/// Load Arrow and Parquet metadata from one local file footer.
fn load_arrow_metadata(file: &Path) -> Result<ArrowReaderMetadata> {
  ArrowReaderMetadata::load(
    &fs::File::open(file).with_context(|| format!("open parquet file: {}", file.display()))?,
    ArrowReaderOptions::new(),
  )
  .with_context(|| format!("read arrow metadata: {}", file.display()))
}

#[cfg(test)]
mod tests {
  use std::fs;

  use tempfile::TempDir;

  use super::{discover_parquet_files, load_arrow_metadata};

  #[test]
  fn parquet_discovery_sorts_files_and_ignores_other_extensions() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join("b.parquet"), []).unwrap();
    fs::write(temp.path().join("a.parquet"), []).unwrap();
    fs::write(temp.path().join("notes.txt"), []).unwrap();

    let files = discover_parquet_files(temp.path()).unwrap().unwrap();

    assert_eq!(
      files,
      vec![temp.path().join("a.parquet"), temp.path().join("b.parquet")]
    );
  }

  #[test]
  fn parquet_discovery_rejects_an_empty_directory() {
    let temp = TempDir::new().unwrap();

    let error = discover_parquet_files(temp.path()).unwrap_err();

    assert!(error.to_string().contains("no parquet files found"));
  }

  #[test]
  fn parquet_discovery_declines_non_parquet_files() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("data.txt");
    fs::write(&path, []).unwrap();

    assert!(discover_parquet_files(&path).unwrap().is_none());
  }

  #[test]
  fn parquet_footer_loading_reports_the_file_context() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("broken.parquet");
    fs::write(&path, b"not parquet").unwrap();

    let error = load_arrow_metadata(&path).unwrap_err();

    assert!(error.to_string().contains("read arrow metadata"));
  }
}

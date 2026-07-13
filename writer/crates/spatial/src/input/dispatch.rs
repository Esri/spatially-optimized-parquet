//! Routes resolved input formats to their concrete source implementation.

use std::sync::Arc;

use anyhow::Result;

use super::{InputOpenOptions, InputSource, SourceFormat, gpkg, parquet};

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

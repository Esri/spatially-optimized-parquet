use std::path::Path;
use std::sync::Arc;

use spatial::input::{InputOpenOptions, InputSource, SourceFormat, open_input};
use tokio::runtime::Runtime;

#[allow(dead_code)]
pub fn open_parquet_input(path: &Path) -> Arc<dyn InputSource> {
  runtime()
    .block_on(open_input(
      SourceFormat::Parquet,
      &InputOpenOptions::new(path.to_path_buf()),
    ))
    .unwrap()
}

#[allow(dead_code)]
pub fn open_gpkg_input(path: &Path, layer: Option<&str>) -> Arc<dyn InputSource> {
  runtime()
    .block_on(open_input(
      SourceFormat::GeoPackage,
      &InputOpenOptions {
        location: path.to_string_lossy().into_owned(),
        layer: layer.map(str::to_string),
      },
    ))
    .unwrap()
}

#[allow(dead_code)]
pub fn runtime() -> Runtime {
  Runtime::new().unwrap()
}

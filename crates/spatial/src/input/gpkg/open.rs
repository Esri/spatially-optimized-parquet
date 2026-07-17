//! Opens local GeoPackage datasets and layers through GDAL's restricted vector interface.
//!
//! Restricts GDAL to the GeoPackage driver and uses immutable, no-lock access for read-only scans.

use std::path::Path;

use anyhow::{Context, Result};
use gdal::vector::OwnedLayer;
use gdal::{Dataset, DatasetOptions, GdalOpenFlags};

const GPKG_ALLOWED_DRIVERS: [&str; 1] = ["GPKG"];
const GPKG_OPEN_OPTIONS: [&str; 2] = ["NOLOCK=YES", "IMMUTABLE=YES"];

pub(super) fn is_gpkg_path(path: &Path) -> bool {
  path.is_file()
    && path
      .extension()
      .is_some_and(|ext| ext.to_string_lossy().eq_ignore_ascii_case("gpkg"))
}

/// Open a GeoPackage with vector-only, immutable, and no-lock GDAL options.
pub(super) fn open_gpkg_dataset(path: &Path) -> Result<Dataset> {
  Dataset::open_ex(
    path,
    DatasetOptions {
      open_flags: GdalOpenFlags::GDAL_OF_VECTOR | GdalOpenFlags::GDAL_OF_VERBOSE_ERROR,
      allowed_drivers: Some(&GPKG_ALLOWED_DRIVERS),
      open_options: Some(&GPKG_OPEN_OPTIONS),
      sibling_files: None,
    },
  )
  .with_context(|| format!("failed to open GeoPackage {}", path.display()))
}

pub(super) fn open_gpkg_layer(path: &Path, layer_name: &str) -> Result<OwnedLayer> {
  open_gpkg_dataset(path)?
    .into_layer_by_name(layer_name)
    .with_context(|| format!("failed to open GeoPackage layer {layer_name}"))
}

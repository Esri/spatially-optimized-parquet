//! Resolves one explicit storage format before source opening begins.
//!
//! Resolution honors a caller override first, treats local directories as Parquet datasets,
//! and otherwise examines the final path extension of a local path or URL. Unknown locations
//! fail with an actionable `--input-format` message rather than falling through source probes.

use std::fmt;
use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use url::Url;

use super::source::is_http_location;

/// Identifies the physical source implementation used to open an input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceFormat {
  /// Selects a GDAL-backed GeoPackage source.
  GeoPackage,
  /// Selects a local or HTTP Parquet source, including GeoParquet.
  Parquet,
}

impl FromStr for SourceFormat {
  type Err = anyhow::Error;

  fn from_str(value: &str) -> Result<Self> {
    match value.to_ascii_lowercase().as_str() {
      "gpkg" | "geopackage" => Ok(Self::GeoPackage),
      "parquet" | "geoparquet" => Ok(Self::Parquet),
      _ => bail!("unsupported input format '{value}'; expected gpkg or parquet"),
    }
  }
}

impl fmt::Display for SourceFormat {
  fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
    formatter.write_str(match self {
      Self::GeoPackage => "gpkg",
      Self::Parquet => "parquet",
    })
  }
}

/// Resolve an input format from an override, directory, local extension, or URL path.
pub fn resolve_source_format(
  location: &str,
  explicit_format: Option<SourceFormat>,
) -> Result<SourceFormat> {
  if let Some(explicit_format) = explicit_format {
    return Ok(explicit_format);
  }

  if !is_http_location(location) && Path::new(location).is_dir() {
    return Ok(SourceFormat::Parquet);
  }

  let extension = if is_http_location(location) {
    let url = Url::parse(location).with_context(|| format!("parse input URL: {location}"))?;
    Path::new(url.path())
      .extension()
      .map(|extension| extension.to_string_lossy().into_owned())
  } else {
    Path::new(location)
      .extension()
      .map(|extension| extension.to_string_lossy().into_owned())
  };

  match extension.as_deref().map(str::to_ascii_lowercase).as_deref() {
    Some("gpkg") => Ok(SourceFormat::GeoPackage),
    Some("parquet") => Ok(SourceFormat::Parquet),
    _ => bail!(
      "unable to determine input format for '{location}'; pass --input-format gpkg or --input-format parquet"
    ),
  }
}

#[cfg(test)]
mod tests {
  use tempfile::TempDir;

  use super::*;

  #[test]
  fn explicit_format_wins_over_extension() {
    assert_eq!(
      resolve_source_format("data.gpkg", Some(SourceFormat::Parquet)).unwrap(),
      SourceFormat::Parquet
    );
  }

  #[test]
  fn resolves_extensions_case_insensitively() {
    assert_eq!(
      resolve_source_format("data.GPKG", None).unwrap(),
      SourceFormat::GeoPackage
    );
    assert_eq!(
      resolve_source_format("data.Parquet", None).unwrap(),
      SourceFormat::Parquet
    );
  }

  #[test]
  fn resolves_directories_as_parquet() {
    let temp = TempDir::new().unwrap();
    assert_eq!(
      resolve_source_format(temp.path().to_str().unwrap(), None).unwrap(),
      SourceFormat::Parquet
    );
  }

  #[test]
  fn resolves_url_path_without_query_string() {
    assert_eq!(
      resolve_source_format("https://example.com/data.parquet?token=value", None).unwrap(),
      SourceFormat::Parquet
    );
  }

  #[test]
  fn unknown_location_requires_override() {
    let error = resolve_source_format("data.current", None).unwrap_err();
    assert!(error.to_string().contains("--input-format"));
  }
}

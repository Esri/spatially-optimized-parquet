//! Owns the serialized GeoParquet and geodisplay metadata contracts.

mod geo;
mod geodisplay;
mod parquet;

use ::parquet::file::metadata::KeyValue;
use anyhow::Result;

use geo::GeoMetadata;
use geodisplay::{GeodisplayMetadata, XzClusteringIndex, ZClusteringIndex};
use parquet::ParquetMetadataSet;

pub(crate) use geo::GeoMetadataInput;
pub(crate) use geodisplay::{MultiscaleLevelInput, XzClusteringIndexInput, ZClusteringIndexInput};

/// Assemble GeoParquet metadata while preserving non-reserved source entries.
pub(crate) fn geoparquet_metadata(
  source_entries: Vec<KeyValue>,
  geo_input: GeoMetadataInput<'_>,
) -> Result<Vec<KeyValue>> {
  let mut metadata = ParquetMetadataSet::new(source_entries);
  metadata.insert(&GeoMetadata::new(geo_input)?)?;
  Ok(metadata.into_entries())
}

/// Assemble GeoParquet and point geodisplay metadata.
pub(crate) fn optimized_point_metadata(
  source_entries: Vec<KeyValue>,
  geo_input: GeoMetadataInput<'_>,
  index_input: ZClusteringIndexInput,
) -> Result<Vec<KeyValue>> {
  optimized_metadata(
    source_entries,
    geo_input,
    GeodisplayMetadata::point(ZClusteringIndex::new(index_input)),
  )
}

/// Assemble GeoParquet and non-point geodisplay metadata.
pub(crate) fn optimized_xz_metadata(
  source_entries: Vec<KeyValue>,
  geo_input: GeoMetadataInput<'_>,
  parent_column: &str,
  index_input: XzClusteringIndexInput,
) -> Result<Vec<KeyValue>> {
  optimized_metadata(
    source_entries,
    geo_input,
    GeodisplayMetadata::xz_with_parent(parent_column, XzClusteringIndex::new(index_input)),
  )
}

fn optimized_metadata(
  source_entries: Vec<KeyValue>,
  geo_input: GeoMetadataInput<'_>,
  geodisplay: GeodisplayMetadata,
) -> Result<Vec<KeyValue>> {
  let mut metadata = ParquetMetadataSet::new(source_entries);
  metadata.insert(&GeoMetadata::new(geo_input)?)?;
  metadata.insert(&geodisplay)?;
  Ok(metadata.into_entries())
}

#[cfg(test)]
mod tests;

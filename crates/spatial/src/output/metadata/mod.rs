//! Owns generic Parquet metadata replacement and Geodisplay metadata contracts.

mod geodisplay;
mod parquet;

use ::parquet::file::metadata::KeyValue;
use anyhow::Result;

use crate::geoparquet::GeoMetadataInput;

pub(crate) use geodisplay::{
  ESRI_PBF_ENCODING, GEODISPLAY_VERSION, GeodisplayIndex, GeodisplayMetadata, MultiscaleLevel,
  QUANTIZED_NATIVE_ENCODING, XzClusteringIndex, ZClusteringIndex,
};
pub(crate) use geodisplay::{MultiscaleLevelInput, XzClusteringIndexInput, ZClusteringIndexInput};
use parquet::ParquetMetadataSet;

/// Assemble GeoParquet and point geodisplay metadata.
pub(crate) fn optimized_point_metadata(
  source_entries: Vec<KeyValue>,
  geo_input: GeoMetadataInput<'_>,
  field: &str,
  index_input: ZClusteringIndexInput,
) -> Result<Vec<KeyValue>> {
  optimized_metadata(
    source_entries,
    geo_input,
    GeodisplayMetadata::point(field, ZClusteringIndex::new(index_input)),
  )
}

/// Assemble GeoParquet and non-point geodisplay metadata.
pub(crate) fn optimized_xz_metadata(
  source_entries: Vec<KeyValue>,
  geo_input: GeoMetadataInput<'_>,
  field: &str,
  index_input: XzClusteringIndexInput,
) -> Result<Vec<KeyValue>> {
  optimized_metadata(
    source_entries,
    geo_input,
    GeodisplayMetadata::xz(field, XzClusteringIndex::new(index_input)),
  )
}

fn optimized_metadata(
  source_entries: Vec<KeyValue>,
  geo_input: GeoMetadataInput<'_>,
  geodisplay: GeodisplayMetadata,
) -> Result<Vec<KeyValue>> {
  let mut metadata = ParquetMetadataSet::new(source_entries);
  metadata.insert_entry(crate::geoparquet::geo_metadata_entry(geo_input)?);
  metadata.insert(&geodisplay)?;
  Ok(metadata.into_entries())
}

#[cfg(test)]
mod tests;

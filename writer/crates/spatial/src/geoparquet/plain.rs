//! Coordinates plain GeoParquet resolution, projection, metadata, and writing.

use anyhow::{Context, Result};
use arrow_schema::Schema;
use datafusion::dataframe::DataFrame;
use engine::{OutputLayout, ParquetWriterOptions, write_single_file};

use crate::input::{InputSource, RowRange};
use crate::optimized::COVERING_BBOX_COLUMN;
use crate::output::{GeoMetadataInput, ReprojectionSpec, geoparquet_metadata};

use super::{
  analyze_plain_target_extent, plain_output_dataframe, resolve_source,
  validate_covering_configuration,
};

pub(crate) struct PlainOutput<'a> {
  input: &'a dyn InputSource,
  input_dataframe: DataFrame,
  output_layout: &'a OutputLayout,
  source_schema: &'a Schema,
  geometry_column: Option<&'a str>,
  input_wkid: Option<u32>,
  row_range: RowRange,
}

impl<'a> PlainOutput<'a> {
  /// Construct plain output coordination for one prepared input selection.
  pub(crate) fn new(
    input: &'a dyn InputSource,
    input_dataframe: DataFrame,
    output_layout: &'a OutputLayout,
    source_schema: &'a Schema,
    geometry_column: Option<&'a str>,
    input_wkid: Option<u32>,
    row_range: RowRange,
  ) -> Self {
    Self {
      input,
      input_dataframe,
      output_layout,
      source_schema,
      geometry_column,
      input_wkid,
      row_range,
    }
  }

  /// Write one plain GeoParquet file from a normalized input and prepared DataFrame.
  pub(crate) async fn write(
    self,
    output_wkid: u32,
    covering: bool,
    compression: Option<&str>,
  ) -> Result<u64> {
    validate_covering_configuration(covering, self.source_schema)?;
    let source = resolve_source(
      self.input,
      self.input_dataframe.clone(),
      self.source_schema,
      self.geometry_column,
      self.input_wkid,
      self.row_range,
    )
    .await?;
    let source_projjson = source
      .source_spatial_reference
      .projjson
      .as_ref()
      .context("missing resolved source CRS PROJJSON")?;
    let reprojection = ReprojectionSpec::from_source_projjson(source_projjson, output_wkid)?;
    let target_extent = analyze_plain_target_extent(
      self.input_dataframe.clone(),
      &source.geometry_spec.column,
      source.geometry_shape.category(),
      reprojection.transform(),
    )
    .await?;
    let dataframe = plain_output_dataframe(
      self.input_dataframe,
      self.source_schema,
      &source.geometry_spec.column,
      source.geometry_shape.category(),
      reprojection.transform(),
      covering,
    )?;
    let geo_metadata = GeoMetadataInput {
      geometry_column: &source.geometry_spec.column,
      geometry_types: &source.geometry_types,
      output_extent: target_extent,
      output_spatial_reference: reprojection.target_spatial_reference(),
      has_z: source.has_z,
      has_m: source.has_m,
      covering,
      covering_column: COVERING_BBOX_COLUMN,
    };
    let metadata = geoparquet_metadata(source.source_metadata.passthrough_kv, geo_metadata)?;
    let writer_options = ParquetWriterOptions::new(compression.unwrap_or("snappy"), &metadata)?;
    let output_path = self
      .output_layout
      .paths()?
      .into_iter()
      .next()
      .context("missing output path")?
      .to_string_lossy()
      .into_owned();

    write_single_file(dataframe, &output_path, writer_options).await
  }
}

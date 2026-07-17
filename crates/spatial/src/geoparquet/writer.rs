//! Coordinates GeoParquet resolution, selection, metadata, and writing.

use anyhow::{Context, Result};
use arrow_schema::Schema;
use datafusion::dataframe::DataFrame;
use datafusion::logical_expr::Expr;
use datafusion::logical_expr::expr_fn::ident;

use crate::input::{InputSource, RowRange};
use crate::optimized::COVERING_BBOX_COLUMN;
use crate::optimized::ExtentResolver;
use crate::output::{OutputLayout, ParquetOutputWriter, ParquetWriterOptions};
use crate::pipeline::SharedWriteReporter;

use super::{
  GeoMetadataInput, NormalizedSpatialFrame, ResolvedReprojection, geoparquet_metadata,
  resolve_source,
};

pub(crate) struct GeoParquetWriter<'a> {
  input: &'a dyn InputSource,
  input_dataframe: DataFrame,
  output_layout: &'a OutputLayout,
  source_schema: &'a Schema,
  geometry_column: Option<&'a str>,
  input_wkid: Option<u32>,
  row_range: RowRange,
  total_rows: u64,
  write_reporter: Option<SharedWriteReporter>,
}

impl<'a> GeoParquetWriter<'a> {
  /// Construct one GeoParquet writer for a prepared input selection.
  pub(crate) fn new(
    input: &'a dyn InputSource,
    input_dataframe: DataFrame,
    output_layout: &'a OutputLayout,
    source_schema: &'a Schema,
    geometry_column: Option<&'a str>,
    input_wkid: Option<u32>,
    row_range: RowRange,
    total_rows: u64,
    write_reporter: Option<SharedWriteReporter>,
  ) -> Self {
    Self {
      input,
      input_dataframe,
      output_layout,
      source_schema,
      geometry_column,
      input_wkid,
      row_range,
      total_rows,
      write_reporter,
    }
  }

  /// Write one GeoParquet file from a normalized input and prepared DataFrame.
  pub(crate) async fn write(
    self,
    output_wkid: u32,
    covering: bool,
    strip_z: bool,
    strip_m: bool,
    compression: Option<&str>,
  ) -> Result<u64> {
    let mut source = resolve_source(
      self.input,
      self.input_dataframe.clone(),
      self.source_schema,
      self.geometry_column,
      self.input_wkid,
      self.row_range,
    )
    .await?;
    source.strip_dimensions(strip_z, strip_m);
    let source_projjson = source
      .source_spatial_reference
      .projjson
      .as_ref()
      .context("missing resolved source CRS PROJJSON")?;
    let reprojection = ResolvedReprojection::from_source_projjson(source_projjson, output_wkid)?;
    let normalized = NormalizedSpatialFrame::new(
      self.input_dataframe.clone(),
      self.source_schema,
      &source,
      &reprojection,
      strip_z,
      strip_m,
    )?;
    let target_extent = ExtentResolver::new(self.input, self.row_range)
      .resolve(&source, &normalized, &reprojection)
      .await?;
    let mut expressions: Vec<Expr> = self
      .source_schema
      .fields()
      .iter()
      .filter(|field| field.name() != COVERING_BBOX_COLUMN)
      .map(|field| ident(field.name()))
      .collect();
    if covering {
      expressions.push(ident(COVERING_BBOX_COLUMN));
    }
    debug_assert!(expressions.iter().any(|expression| {
      matches!(expression, Expr::Column(column) if column.name == source.geometry.column)
    }));
    let dataframe = normalized.dataframe().select(expressions)?;
    let geo_metadata = GeoMetadataInput {
      geometry_column: &source.geometry.column,
      geometry_types: &source.geometry_types,
      output_extent: target_extent,
      output_spatial_reference: reprojection.target_spatial_reference(),
      has_z: source.has_z,
      has_m: source.has_m,
      covering,
      covering_column: COVERING_BBOX_COLUMN,
    };
    let metadata = geoparquet_metadata(source.source_metadata.passthrough_kv, geo_metadata)?;
    let writer_options =
      ParquetWriterOptions::new(compression.unwrap_or("snappy"), &metadata)?.into_datafusion();
    let output_path = self
      .output_layout
      .paths()?
      .into_iter()
      .next()
      .context("missing output path")?
      .to_string_lossy()
      .into_owned();
    ParquetOutputWriter::new(self.total_rows, self.write_reporter)
      .write_single(dataframe, output_path, writer_options, Vec::new())
      .await
  }
}

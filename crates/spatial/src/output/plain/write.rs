//! Selects and writes plain GeoParquet output.

use anyhow::{Context, Result};
use arrow_schema::Schema;
use datafusion::logical_expr::Expr;
use datafusion::logical_expr::expr_fn::ident;

use crate::output::{OutputPath, Writer, WriterOptions};
use crate::pipeline::SharedWriteReporter;

use crate::geoparquet::{COVERING_BBOX_COLUMN, GeoMetadata, GeoMetadataInput};
use crate::pipeline::SpatialWriteContext;

pub(crate) struct PlainWriter<'a> {
  context: &'a SpatialWriteContext,
  output_path: &'a OutputPath,
  source_schema: &'a Schema,
  total_rows: u64,
  write_reporter: Option<SharedWriteReporter>,
}

impl<'a> PlainWriter<'a> {
  /// Construct one plain GeoParquet writer for prepared spatial data.
  pub(crate) fn new(
    context: &'a SpatialWriteContext,
    output_path: &'a OutputPath,
    source_schema: &'a Schema,
    total_rows: u64,
    write_reporter: Option<SharedWriteReporter>,
  ) -> Self {
    Self {
      context,
      output_path,
      source_schema,
      total_rows,
      write_reporter,
    }
  }

  /// Write one GeoParquet file from prepared spatial data.
  pub(crate) async fn write(self, covering: bool, compression: Option<&str>) -> Result<u64> {
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
      matches!(expression, Expr::Column(column) if column.name == self.context.source().geometry.column)
    }));
    let dataframe = self.context.frame().dataframe().select(expressions)?;
    let geo_metadata = GeoMetadataInput {
      geometry_column: &self.context.source().geometry.column,
      geometry_types: &self.context.source().geometry_types,
      output_extent: self.context.target_extent(),
      output_spatial_reference: self.context.reprojection().target_spatial_reference(),
      has_z: self.context.source().has_z,
      has_m: self.context.source().has_m,
      covering,
      covering_column: COVERING_BBOX_COLUMN,
      ordering: None,
      lod: None,
    };
    let metadata = GeoMetadata::parquet_entries(
      self.context.source().source_metadata.passthrough_kv.clone(),
      geo_metadata,
    )?;
    let geometry_column = &self.context.source().geometry.column;
    let geometry_crs = format!(
      "srid:{}",
      self
        .context
        .reprojection()
        .target_spatial_reference()
        .wkid
        .context("missing output spatial-reference WKID")?
    );
    let writer_options = WriterOptions::new(compression.unwrap_or("snappy"), &metadata)?
      .with_geometry_column(geometry_column, geometry_crs);
    let output_path = self
      .output_path
      .paths()?
      .into_iter()
      .next()
      .context("missing output path")?
      .to_string_lossy()
      .into_owned();
    Writer::new(self.total_rows, self.write_reporter)
      .write_single(dataframe, output_path, writer_options, Vec::new())
      .await
  }
}

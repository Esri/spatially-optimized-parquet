//! Writes GeoParquet without SOP display columns or spatial sorting.
//!
//! Plain output preserves source WKB coordinates and CRS, regenerates GeoParquet metadata for
//! the selected rows, and can add a GeoParquet 1.1 covering bbox. Data remains a lazy DataFusion
//! plan until DataFusion writes the final Parquet file.

use anyhow::{Context, Result, bail};
use datafusion::logical_expr::expr_fn::ident;
use engine::plan::{OutputPlan, output_paths};
use engine::write::{create_datafusion_parquet_options, parse_compression};

use crate::input::{InputSource, RowRange};
use crate::output::geoparquet::{
  build_geo_key_values, build_geo_metadata, resolve_context, validate_covering_configuration,
};
use crate::output::optimized::multiscale::{
  COVERING_BBOX_COLUMN, TEMP_XMAX_COLUMN, TEMP_XMIN_COLUMN, TEMP_YMAX_COLUMN, TEMP_YMIN_COLUMN,
};
use crate::udf::{
  bounds_xmax_expr, bounds_xmin_expr, bounds_ymax_expr, bounds_ymin_expr, feature_bbox_expr,
};

use super::write::write_dataframe;

/// Carries the source and writer controls required by plain GeoParquet output.
pub struct PlainOutputRequest<'a> {
  /// Supplies the normalized source.
  pub input: &'a dyn InputSource,
  /// Supplies the configured DataFusion session.
  pub session: &'a engine::SessionContext,
  /// Supplies the validated one-file output plan.
  pub output_plan: &'a OutputPlan,
  /// Selects source rows.
  pub row_range: RowRange,
  /// Reuses materialized HTTP batches when available.
  pub materialized_batches: Option<&'a [arrow_array::RecordBatch]>,
  /// Overrides geometry-column inference.
  pub geometry_column: Option<&'a str>,
  /// Supplies a missing source CRS.
  pub input_wkid: Option<u32>,
  /// Enables a GeoParquet covering bbox.
  pub covering: bool,
  /// Selects Parquet compression.
  pub compression: Option<&'a str>,
}

/// Write selected rows as normalized, unsorted GeoParquet.
pub async fn write(request: PlainOutputRequest<'_>) -> Result<u64> {
  if request.output_plan.parts != 1 {
    bail!("plain GeoParquet output does not support --output-files");
  }
  let schema = request.input.schema()?;
  validate_covering_configuration(request.covering, schema.as_ref())?;
  let context = resolve_context(
    request.input,
    schema.as_ref(),
    request.geometry_column,
    request.input_wkid,
    request.row_range,
  )
  .await?;
  let dataframe = if let Some(batches) = request.materialized_batches {
    request.session.read_batches(batches.iter().cloned())?
  } else {
    request
      .input
      .to_dataframe(request.session, request.row_range)
      .await?
  };
  let dataframe = if request.covering {
    add_covering_column(dataframe, schema.as_ref(), &context.geometry_spec.column)?
  } else {
    dataframe
  };
  let geo_metadata = build_geo_metadata(
    &context.geometry_spec.column,
    &context.geometry_types,
    context.full_extent,
    &context.spatial_reference,
    context.has_z,
    context.has_m,
    request.covering,
    COVERING_BBOX_COLUMN,
  )?;
  let metadata = build_geo_key_values(&context.source_metadata, geo_metadata);
  let compression = parse_compression(request.compression.unwrap_or("snappy"))?;
  let writer_options = create_datafusion_parquet_options(compression, &metadata);
  let output_path = output_paths(request.output_plan)?
    .into_iter()
    .next()
    .context("missing output path")?
    .to_string_lossy()
    .into_owned();
  write_dataframe(dataframe, &output_path, writer_options).await
}

fn add_covering_column(
  mut dataframe: engine::DataFrame,
  source_schema: &arrow_schema::Schema,
  geometry_column: &str,
) -> Result<engine::DataFrame> {
  dataframe = dataframe.with_column(TEMP_XMIN_COLUMN, bounds_xmin_expr(geometry_column))?;
  dataframe = dataframe.with_column(TEMP_YMIN_COLUMN, bounds_ymin_expr(geometry_column))?;
  dataframe = dataframe.with_column(TEMP_XMAX_COLUMN, bounds_xmax_expr(geometry_column))?;
  dataframe = dataframe.with_column(TEMP_YMAX_COLUMN, bounds_ymax_expr(geometry_column))?;
  let expressions: Vec<datafusion::logical_expr::Expr> = source_schema
    .fields()
    .iter()
    .map(|field| ident(field.name()))
    .chain(std::iter::once(feature_bbox_expr(
      geometry_column,
      TEMP_XMIN_COLUMN,
      TEMP_YMIN_COLUMN,
      TEMP_XMAX_COLUMN,
      TEMP_YMAX_COLUMN,
    )))
    .collect();
  dataframe.select(expressions).map_err(Into::into)
}

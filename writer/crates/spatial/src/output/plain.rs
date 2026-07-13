//! Writes GeoParquet without SOP display columns or spatial sorting.
//!
//! Plain output projects WKB into the requested CRS, regenerates GeoParquet metadata for the
//! selected rows, and can add a GeoParquet 1.1 covering bbox. Data remains a lazy DataFusion plan
//! until DataFusion writes the final Parquet file.

use anyhow::{Context, Result, bail};
use arrow_array::{Array, Float64Array, RecordBatch};
use datafusion::functions::core::expr_ext::FieldAccessor;
use datafusion::functions_aggregate::expr_fn::{max, min};
use datafusion::logical_expr::Expr;
use datafusion::logical_expr::expr_fn::ident;
use engine::output_layout::{OutputLayout, resolved_output_paths};
use engine::write::{create_datafusion_parquet_options, parse_compression};

use super::write::write_dataframe;
use crate::analysis::{DisplayGeometryType, Extent2D, GeometryFamily};
use crate::input::{InputSource, RowRange};
use crate::output::geoparquet::{
  build_geo_key_values, build_geo_metadata, feature_bbox_expr, resolve_source_context,
  validate_covering_configuration,
};
use crate::output::optimized::clustering::{bounds_expr, point_expr};
use crate::output::optimized::multiscale::{
  COVERING_BBOX_COLUMN, TEMP_BOUNDS_COLUMN, TEMP_POINT_COORDS_COLUMN,
  TEMP_REPROJECTED_GEOMETRY_COLUMN, TEMP_XMAX_COLUMN, TEMP_XMIN_COLUMN, TEMP_YMAX_COLUMN,
  TEMP_YMIN_COLUMN,
};
use crate::output::reprojection::{
  ReprojectionPlan, TransformSpec, reproject_geometry_expr, transformed_bounds_expr,
  transformed_point_coords_expr,
};

/// Carries the source and writer controls required by plain GeoParquet output.
pub struct PlainOutputRequest<'a> {
  /// Supplies the normalized source.
  pub input: &'a dyn InputSource,
  /// Supplies the configured DataFusion session.
  pub session: &'a engine::SessionContext,
  /// Supplies the resolved one-file output layout.
  pub output_layout: &'a OutputLayout,
  /// Selects source rows.
  pub row_range: RowRange,
  /// Reuses materialized HTTP batches when available.
  pub materialized_batches: Option<&'a [arrow_array::RecordBatch]>,
  /// Overrides geometry-column inference.
  pub geometry_column: Option<&'a str>,
  /// Supplies a missing source CRS.
  pub input_wkid: Option<u32>,
  /// Selects the output spatial reference.
  pub output_wkid: u32,
  /// Enables a GeoParquet covering bbox.
  pub covering: bool,
  /// Selects Parquet compression.
  pub compression: Option<&'a str>,
}

/// Write selected rows as normalized, unsorted GeoParquet.
pub async fn write(request: PlainOutputRequest<'_>) -> Result<u64> {
  if request.output_layout.parts != 1 {
    bail!("plain GeoParquet output does not support --output-files");
  }
  let schema = request.input.schema()?;
  validate_covering_configuration(request.covering, schema.as_ref())?;
  let source_context = resolve_source_context(
    request.input,
    schema.as_ref(),
    request.geometry_column,
    request.input_wkid,
    request.row_range,
  )
  .await?;
  let source_dataframe = if let Some(batches) = request.materialized_batches {
    request.session.read_batches(batches.iter().cloned())?
  } else {
    request
      .input
      .to_dataframe(request.session, request.row_range)
      .await?
  };
  let reprojection = ReprojectionPlan::from_source_metadata(
    &source_context.source_metadata,
    &source_context.geometry_spec.column,
    request.output_wkid,
  )?;
  let target_extent = analyze_target_extent(
    source_dataframe.clone(),
    &source_context.geometry_spec.column,
    source_context.geometry_type,
    reprojection.transform(),
  )
  .await?;
  let dataframe = build_output_dataframe(
    source_dataframe,
    schema.as_ref(),
    &source_context.geometry_spec.column,
    source_context.geometry_type,
    reprojection.transform(),
    request.covering,
  )?;
  let geo_metadata = build_geo_metadata(
    &source_context.geometry_spec.column,
    &source_context.geometry_types,
    target_extent,
    reprojection.target_spatial_reference(),
    source_context.has_z,
    source_context.has_m,
    request.covering,
    COVERING_BBOX_COLUMN,
  )?;
  let metadata = build_geo_key_values(&source_context.source_metadata, geo_metadata);
  let compression = parse_compression(request.compression.unwrap_or("snappy"))?;
  let writer_options = create_datafusion_parquet_options(compression, &metadata);
  let output_path = resolved_output_paths(request.output_layout)?
    .into_iter()
    .next()
    .context("missing output path")?
    .to_string_lossy()
    .into_owned();
  write_dataframe(dataframe, &output_path, writer_options).await
}

async fn analyze_target_extent(
  dataframe: engine::DataFrame,
  geometry_column: &str,
  geometry_type: DisplayGeometryType,
  transform: Option<&TransformSpec>,
) -> Result<Extent2D> {
  let aggregate_dataframe =
    add_target_coordinate_columns(dataframe, geometry_column, geometry_type, transform)?
      .aggregate(
        vec![],
        vec![
          min(ident(TEMP_XMIN_COLUMN)).alias(TEMP_XMIN_COLUMN),
          min(ident(TEMP_YMIN_COLUMN)).alias(TEMP_YMIN_COLUMN),
          max(ident(TEMP_XMAX_COLUMN)).alias(TEMP_XMAX_COLUMN),
          max(ident(TEMP_YMAX_COLUMN)).alias(TEMP_YMAX_COLUMN),
        ],
      )?;
  let batches = aggregate_dataframe.collect().await?;
  extract_extent(&batches)
}

fn build_output_dataframe(
  mut dataframe: engine::DataFrame,
  source_schema: &arrow_schema::Schema,
  geometry_column: &str,
  geometry_type: DisplayGeometryType,
  transform: Option<&TransformSpec>,
  covering: bool,
) -> Result<engine::DataFrame> {
  if covering {
    dataframe =
      add_target_coordinate_columns(dataframe, geometry_column, geometry_type, transform)?;
  }
  let output_geometry_column = if let Some(transform) = transform {
    dataframe = dataframe.with_column(
      TEMP_REPROJECTED_GEOMETRY_COLUMN,
      reproject_geometry_expr(geometry_column, transform),
    )?;
    TEMP_REPROJECTED_GEOMETRY_COLUMN
  } else {
    geometry_column
  };
  let mut expressions: Vec<Expr> = source_schema
    .fields()
    .iter()
    .map(|field| {
      if field.name() == geometry_column {
        ident(output_geometry_column).alias(geometry_column)
      } else {
        ident(field.name())
      }
    })
    .collect();
  if covering {
    expressions.push(feature_bbox_expr(
      output_geometry_column,
      TEMP_XMIN_COLUMN,
      TEMP_YMIN_COLUMN,
      TEMP_XMAX_COLUMN,
      TEMP_YMAX_COLUMN,
    ));
  }
  dataframe.select(expressions).map_err(Into::into)
}

fn add_target_coordinate_columns(
  mut dataframe: engine::DataFrame,
  geometry_column: &str,
  geometry_type: DisplayGeometryType,
  transform: Option<&TransformSpec>,
) -> Result<engine::DataFrame> {
  match geometry_type.family() {
    GeometryFamily::Point => {
      let point_coordinates = match transform {
        Some(transform) => transformed_point_coords_expr(geometry_column, transform),
        None => point_expr(geometry_column),
      };
      dataframe = dataframe.with_column(TEMP_POINT_COORDS_COLUMN, point_coordinates)?;
      dataframe =
        dataframe.with_column(TEMP_XMIN_COLUMN, ident(TEMP_POINT_COORDS_COLUMN).field("x"))?;
      dataframe =
        dataframe.with_column(TEMP_YMIN_COLUMN, ident(TEMP_POINT_COORDS_COLUMN).field("y"))?;
      dataframe = dataframe.with_column(TEMP_XMAX_COLUMN, ident(TEMP_XMIN_COLUMN))?;
      dataframe = dataframe.with_column(TEMP_YMAX_COLUMN, ident(TEMP_YMIN_COLUMN))?;
    }
    GeometryFamily::NonPoint => {
      let bounds = match transform {
        Some(transform) => transformed_bounds_expr(geometry_column, geometry_type, transform),
        None => bounds_expr(geometry_column),
      };
      dataframe = dataframe.with_column(TEMP_BOUNDS_COLUMN, bounds)?;
      dataframe =
        dataframe.with_column(TEMP_XMIN_COLUMN, ident(TEMP_BOUNDS_COLUMN).field("xmin"))?;
      dataframe =
        dataframe.with_column(TEMP_YMIN_COLUMN, ident(TEMP_BOUNDS_COLUMN).field("ymin"))?;
      dataframe =
        dataframe.with_column(TEMP_XMAX_COLUMN, ident(TEMP_BOUNDS_COLUMN).field("xmax"))?;
      dataframe =
        dataframe.with_column(TEMP_YMAX_COLUMN, ident(TEMP_BOUNDS_COLUMN).field("ymax"))?;
    }
  }
  Ok(dataframe)
}

fn extract_extent(batches: &[RecordBatch]) -> Result<Extent2D> {
  let batch = batches
    .first()
    .context("unable to determine plain GeoParquet extent")?;
  Ok(Extent2D {
    xmin: aggregate_value(batch, 0, "xmin")?,
    ymin: aggregate_value(batch, 1, "ymin")?,
    xmax: aggregate_value(batch, 2, "xmax")?,
    ymax: aggregate_value(batch, 3, "ymax")?,
  })
}

fn aggregate_value(batch: &RecordBatch, column_index: usize, label: &str) -> Result<f64> {
  let values = batch
    .column(column_index)
    .as_any()
    .downcast_ref::<Float64Array>()
    .with_context(|| format!("plain GeoParquet aggregate column '{label}' was not Float64"))?;
  if values.is_null(0) {
    bail!("unable to determine plain GeoParquet extent");
  }
  Ok(values.value(0))
}

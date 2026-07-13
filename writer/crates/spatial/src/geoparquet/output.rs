//! Writes GeoParquet without SOP display columns or spatial sorting.

use anyhow::{Context, Result, bail};
use arrow_array::{Array, Float64Array, RecordBatch};
use async_trait::async_trait;
use datafusion::functions::core::expr_ext::FieldAccessor;
use datafusion::functions_aggregate::expr_fn::{max, min};
use datafusion::logical_expr::Expr;
use datafusion::logical_expr::expr_fn::ident;
use engine::output_layout::resolved_output_paths;
use engine::write::{create_datafusion_parquet_options, parse_compression};

use crate::geometry::{Extent2D, GeometryCategory};
use crate::geoparquet::{
  GeoMetadataInput, build_geo_key_values, build_geo_metadata, feature_bbox_expr,
  resolve_source_context, validate_covering_configuration,
};
use crate::optimized::clustering::{bounds_expr, point_expr};
use crate::optimized::multiscale::{
  COVERING_BBOX_COLUMN, TEMP_BOUNDS_COLUMN, TEMP_POINT_COORDS_COLUMN,
  TEMP_REPROJECTED_GEOMETRY_COLUMN, TEMP_XMAX_COLUMN, TEMP_XMIN_COLUMN, TEMP_YMAX_COLUMN,
  TEMP_YMIN_COLUMN,
};
use crate::output::reprojection::{
  ReprojectionContext, TransformSpec, reproject_geometry_expr, transformed_bounds_expr,
  transformed_point_coords_expr,
};
use crate::output::stage::{OutputStage, OutputStageContext, OutputStageResult};
use crate::output::write::write_dataframe;

/// Provides normalized, unsorted GeoParquet through the shared output-stage boundary.
pub(crate) struct PlainGeoParquet;

#[async_trait]
impl OutputStage for PlainGeoParquet {
  async fn execute(&self, context: OutputStageContext<'_>) -> Result<OutputStageResult> {
    if context.output_layout.parts != 1 {
      bail!("plain GeoParquet output does not support --output-files");
    }
    validate_covering_configuration(context.covering, context.source_schema)?;
    let source_context = resolve_source_context(
      context.input,
      context.source_schema,
      context.geometry_column,
      context.input_wkid,
      context.row_range,
    )
    .await?;
    let source_dataframe = if let Some(batches) = context.materialized_batches {
      context.session.read_batches(batches.iter().cloned())?
    } else {
      context
        .input
        .to_dataframe(context.session, context.row_range)
        .await?
    };
    let source_projjson = source_context
      .source_spatial_reference
      .projjson
      .as_ref()
      .context("missing resolved source CRS PROJJSON")?;
    let reprojection_context =
      ReprojectionContext::from_source_projjson(source_projjson, context.output_wkid)?;
    let target_extent = analyze_target_extent(
      source_dataframe.clone(),
      &source_context.geometry_spec.column,
      source_context.geometry_shape.category(),
      reprojection_context.transform(),
    )
    .await?;
    let dataframe = build_output_dataframe(
      source_dataframe,
      context.source_schema,
      &source_context.geometry_spec.column,
      source_context.geometry_shape.category(),
      reprojection_context.transform(),
      context.covering,
    )?;
    let geo_metadata = build_geo_metadata(GeoMetadataInput {
      geometry_column: &source_context.geometry_spec.column,
      geometry_types: &source_context.geometry_types,
      output_extent: target_extent,
      output_spatial_reference: reprojection_context.target_spatial_reference(),
      has_z: source_context.has_z,
      has_m: source_context.has_m,
      covering: context.covering,
      covering_column: COVERING_BBOX_COLUMN,
    })?;
    let metadata = build_geo_key_values(&source_context.source_metadata, geo_metadata);
    let compression = parse_compression(context.compression.unwrap_or("snappy"))?;
    let writer_options = create_datafusion_parquet_options(compression, &metadata);
    let output_path = resolved_output_paths(context.output_layout)?
      .into_iter()
      .next()
      .context("missing output path")?
      .to_string_lossy()
      .into_owned();
    let rows_written = write_dataframe(dataframe, &output_path, writer_options).await?;
    Ok(OutputStageResult { rows_written })
  }
}

async fn analyze_target_extent(
  dataframe: engine::DataFrame,
  geometry_column: &str,
  geometry_category: GeometryCategory,
  transform: Option<&TransformSpec>,
) -> Result<Extent2D> {
  let aggregate_dataframe =
    add_target_coordinate_columns(dataframe, geometry_column, geometry_category, transform)?
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
  geometry_category: GeometryCategory,
  transform: Option<&TransformSpec>,
  covering: bool,
) -> Result<engine::DataFrame> {
  if covering {
    dataframe =
      add_target_coordinate_columns(dataframe, geometry_column, geometry_category, transform)?;
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
  geometry_category: GeometryCategory,
  transform: Option<&TransformSpec>,
) -> Result<engine::DataFrame> {
  match geometry_category {
    GeometryCategory::Point => {
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
    GeometryCategory::NonPoint => {
      let bounds = match transform {
        Some(transform) => transformed_bounds_expr(geometry_column, geometry_category, transform),
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

//! Writes GeoParquet without optimized clustering columns or spatial sorting.

use anyhow::{Context, Result, bail};
use arrow_array::{Array, Float64Array, RecordBatch};
use datafusion::functions::core::expr_ext::FieldAccessor;
use datafusion::functions_aggregate::expr_fn::{max, min};
use datafusion::logical_expr::Expr;
use datafusion::logical_expr::expr_fn::ident;

use crate::geometry::{Extent2D, GeometryCategory};
use crate::geoparquet::feature_bbox_expr;
use crate::optimized::clustering::{bounds_expr, point_expr};
use crate::optimized::multiscale::{
  TEMP_BOUNDS_COLUMN, TEMP_POINT_COORDS_COLUMN, TEMP_REPROJECTED_GEOMETRY_COLUMN, TEMP_XMAX_COLUMN,
  TEMP_XMIN_COLUMN, TEMP_YMAX_COLUMN, TEMP_YMIN_COLUMN,
};
use crate::output::reprojection::{
  CoordinateTransformSpec, reproject_geometry_expr, transformed_bounds_expr,
  transformed_point_coords_expr,
};

pub(crate) async fn analyze_plain_target_extent(
  dataframe: engine::DataFrame,
  geometry_column: &str,
  geometry_category: GeometryCategory,
  transform: Option<&CoordinateTransformSpec>,
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

pub(crate) fn plain_output_dataframe(
  mut dataframe: engine::DataFrame,
  source_schema: &arrow_schema::Schema,
  geometry_column: &str,
  geometry_category: GeometryCategory,
  transform: Option<&CoordinateTransformSpec>,
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
  transform: Option<&CoordinateTransformSpec>,
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

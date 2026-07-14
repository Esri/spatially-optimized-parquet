//! Normalizes spatial columns before extent analysis and output projection.
//!
//! Reprojects geometry when required, creates one canonical GeoParquet bbox column, and records
//! whether compatible point optimization columns can be reused by optimized output.

use anyhow::Result;
use arrow_schema::{DataType, Schema};
use datafusion::dataframe::DataFrame;
use datafusion::logical_expr::expr_fn::ident;

use crate::geoparquet::{ResolvedGeoParquetSource, geometry_bbox_expr, point_bbox_expr};
use crate::input::SourcePointOptimizationMetadata;
use crate::optimized::{
  COVERING_BBOX_COLUMN, DEFAULT_COORDINATE_PRECISION, POINT_X_COLUMN, POINT_Y_COLUMN,
  POINT_Z_CODE_COLUMN,
};
use crate::output::{ReprojectionSpec, reproject_geometry_expr};

#[derive(Clone)]
pub(crate) struct PreparedSpatialFrame {
  dataframe: DataFrame,
  geometry_column: String,
  point_optimization_reused: bool,
}

impl PreparedSpatialFrame {
  pub(crate) fn new(
    mut dataframe: DataFrame,
    source_schema: &Schema,
    source: &ResolvedGeoParquetSource,
    reprojection: &ReprojectionSpec,
  ) -> Result<Self> {
    if let Some(transform) = reprojection.transform() {
      dataframe = dataframe.with_column(
        &source.geometry_spec.column,
        reproject_geometry_expr(&source.geometry_spec.column, transform),
      )?;
    }

    let reusable_covering = (!reprojection.requires_reprojection())
      .then(|| {
        source
          .source_metadata
          .geometry
          .as_ref()
          .and_then(|geometry| geometry.covering.as_ref())
      })
      .flatten()
      .filter(|covering| valid_bbox_field(source_schema, &covering.column));
    let reusable_point_optimization =
      source
        .source_metadata
        .point_optimization
        .as_ref()
        .filter(|optimization| {
          point_optimization_is_compatible(source_schema, source, reprojection, optimization)
        });
    dataframe = if let Some(covering) = reusable_covering {
      dataframe.with_column(COVERING_BBOX_COLUMN, ident(&covering.column))?
    } else if let Some(optimization) = reusable_point_optimization {
      dataframe.with_column(
        COVERING_BBOX_COLUMN,
        point_bbox_expr(
          &source.geometry_spec.column,
          &optimization.x_column,
          &optimization.y_column,
        ),
      )?
    } else {
      dataframe.with_column(
        COVERING_BBOX_COLUMN,
        geometry_bbox_expr(
          &source.geometry_spec.column,
          source.geometry_shape.category(),
        ),
      )?
    };

    Ok(Self {
      dataframe,
      geometry_column: source.geometry_spec.column.clone(),
      point_optimization_reused: reusable_point_optimization.is_some(),
    })
  }

  pub(crate) fn dataframe(&self) -> DataFrame {
    self.dataframe.clone()
  }

  pub(crate) fn geometry_column(&self) -> &str {
    &self.geometry_column
  }

  pub(crate) fn point_optimization_reused(&self) -> bool {
    self.point_optimization_reused
  }
}

fn valid_bbox_field(schema: &Schema, column: &str) -> bool {
  let Ok(field) = schema.field_with_name(column) else {
    return false;
  };
  let DataType::Struct(fields) = field.data_type() else {
    return false;
  };
  ["xmin", "ymin", "xmax", "ymax"].into_iter().all(|name| {
    fields
      .find(name)
      .is_some_and(|(_, field)| field.data_type() == &DataType::Float64)
  })
}

fn point_optimization_is_compatible(
  schema: &Schema,
  source: &ResolvedGeoParquetSource,
  reprojection: &ReprojectionSpec,
  optimization: &SourcePointOptimizationMetadata,
) -> bool {
  !reprojection.requires_reprojection()
    && source.geometry_shape.category() == crate::geometry::GeometryCategory::Point
    && optimization.code == POINT_Z_CODE_COLUMN
    && optimization.x_column == POINT_X_COLUMN
    && optimization.y_column == POINT_Y_COLUMN
    && optimization.coordinate_precision == DEFAULT_COORDINATE_PRECISION
    && optimization.full_extent == source.source_extent
    && optimization.wkid == reprojection.target_spatial_reference().wkid
    && valid_scalar_field(schema, &optimization.code, &DataType::UInt64)
    && valid_scalar_field(schema, &optimization.x_column, &DataType::Float64)
    && valid_scalar_field(schema, &optimization.y_column, &DataType::Float64)
}

fn valid_scalar_field(schema: &Schema, column: &str, data_type: &DataType) -> bool {
  schema
    .field_with_name(column)
    .is_ok_and(|field| field.data_type() == data_type && !field.is_nullable())
}

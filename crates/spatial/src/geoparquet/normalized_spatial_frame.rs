//! Normalizes spatial columns before extent analysis and output projection.
//!
//! Reprojects geometry when required, creates one canonical GeoParquet bbox column, and records
//! the canonical geometry state consumed by optimized output.

use anyhow::Result;
use arrow_schema::{DataType, Schema};
use datafusion::dataframe::DataFrame;
use datafusion::logical_expr::expr_fn::ident;

use crate::geoparquet::{
  ResolvedGeoParquetSource, ResolvedReprojection, geometry_bbox_expr, reproject_geometry_expr,
};
use crate::optimized::COVERING_BBOX_COLUMN;
use crate::output::strip_geometry_dimensions_expr;

#[derive(Clone)]
pub(crate) struct NormalizedSpatialFrame {
  dataframe: DataFrame,
  geometry_column: String,
}

impl NormalizedSpatialFrame {
  pub(crate) fn new(
    mut dataframe: DataFrame,
    source_schema: &Schema,
    source: &ResolvedGeoParquetSource,
    reprojection: &ResolvedReprojection,
    strip_z: bool,
    strip_m: bool,
  ) -> Result<Self> {
    if let Some(expression) = reproject_geometry_expr(&source.geometry.column, reprojection)? {
      dataframe = dataframe.with_column(&source.geometry.column, expression)?;
    }
    if strip_z || strip_m {
      dataframe = dataframe.with_column(
        &source.geometry.column,
        strip_geometry_dimensions_expr(&source.geometry.column, strip_z, strip_m),
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
    dataframe = if let Some(covering) = reusable_covering {
      dataframe.with_column(COVERING_BBOX_COLUMN, ident(&covering.column))?
    } else {
      dataframe.with_column(
        COVERING_BBOX_COLUMN,
        geometry_bbox_expr(&source.geometry.column, source.geometry_type),
      )?
    };

    Ok(Self {
      dataframe,
      geometry_column: source.geometry.column.clone(),
    })
  }

  pub(crate) fn dataframe(&self) -> DataFrame {
    self.dataframe.clone()
  }

  pub(crate) fn geometry_column(&self) -> &str {
    &self.geometry_column
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

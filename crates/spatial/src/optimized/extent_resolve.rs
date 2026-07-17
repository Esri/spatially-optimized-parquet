//! Resolves the optimized extent in target coordinates.

use anyhow::{Context, Result, bail};
use arrow_array::{Array, Float64Array, RecordBatch};
use datafusion::dataframe::DataFrame;
use datafusion::functions_aggregate::expr_fn::{max, min};

use crate::geometry::Extent2D;
use crate::geoparquet::{
  NormalizedSpatialFrame, ResolvedGeoParquetSource, ResolvedReprojection, bbox_field_expr,
};
use crate::input::{InputSource, RowRange};
use crate::plan_diagnostics::collect_dataframe;

const EXTENT_XMIN_COLUMN: &str = "__extent_xmin";
const EXTENT_YMIN_COLUMN: &str = "__extent_ymin";
const EXTENT_XMAX_COLUMN: &str = "__extent_xmax";
const EXTENT_YMAX_COLUMN: &str = "__extent_ymax";

/// Resolves one selected dataset extent in the output coordinate reference system.
pub(crate) struct ExtentResolver<'a> {
  input: &'a dyn InputSource,
  row_range: RowRange,
}

impl<'a> ExtentResolver<'a> {
  /// Construct target-extent resolution for one prepared input selection.
  pub(crate) fn new(input: &'a dyn InputSource, row_range: RowRange) -> Self {
    Self { input, row_range }
  }

  /// Resolve the selected-row extent in the output coordinate reference system.
  pub(crate) async fn resolve(
    self,
    source: &ResolvedGeoParquetSource,
    normalized: &NormalizedSpatialFrame,
    reprojection: &ResolvedReprojection,
  ) -> Result<Extent2D> {
    let source_metadata = self.input.source_metadata()?;
    let metadata_fast_path = self.row_range.is_full()
      && !reprojection.requires_reprojection()
      && source_metadata
        .geometry
        .as_ref()
        .filter(|metadata| metadata.column == normalized.geometry_column())
        .and_then(|metadata| metadata.bbox)
        .is_some();
    let target_extent = if metadata_fast_path {
      source.source_extent
    } else {
      let aggregate_dataframe = Self::target_extent_aggregate(normalized.dataframe())?;
      let batches = collect_dataframe(aggregate_dataframe, "target extent aggregate").await?;
      Self::extract_target_extent(&batches)?
    };
    Ok(target_extent)
  }

  fn target_extent_aggregate(dataframe: DataFrame) -> Result<DataFrame> {
    dataframe
      .aggregate(
        vec![],
        vec![
          min(bbox_field_expr("xmin")).alias(EXTENT_XMIN_COLUMN),
          min(bbox_field_expr("ymin")).alias(EXTENT_YMIN_COLUMN),
          max(bbox_field_expr("xmax")).alias(EXTENT_XMAX_COLUMN),
          max(bbox_field_expr("ymax")).alias(EXTENT_YMAX_COLUMN),
        ],
      )
      .map_err(Into::into)
  }

  fn extract_target_extent(batches: &[RecordBatch]) -> Result<Extent2D> {
    let Some(batch) = batches.first().filter(|batch| batch.num_rows() > 0) else {
      bail!("unable to determine dataset target extent");
    };
    Ok(Extent2D {
      xmin: Self::extract_aggregate_value(batch, 0, "xmin")?,
      ymin: Self::extract_aggregate_value(batch, 1, "ymin")?,
      xmax: Self::extract_aggregate_value(batch, 2, "xmax")?,
      ymax: Self::extract_aggregate_value(batch, 3, "ymax")?,
    })
  }

  fn extract_aggregate_value(batch: &RecordBatch, column_index: usize, label: &str) -> Result<f64> {
    let values = batch
      .column(column_index)
      .as_any()
      .downcast_ref::<Float64Array>()
      .with_context(|| format!("target extent aggregate column '{label}' was not Float64"))?;
    if values.is_null(0) {
      bail!("unable to determine dataset target extent");
    }
    Ok(values.value(0))
  }
}

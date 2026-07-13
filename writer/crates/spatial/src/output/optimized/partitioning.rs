//! Computes balanced spatial-code ranges for exact multi-file output.

use anyhow::{Context, Result};
use arrow_array::{Array, UInt64Array};
use datafusion::functions_aggregate::approx_percentile_cont::approx_percentile_cont;
use datafusion::functions_aggregate::expr_fn::min;
use datafusion::logical_expr::expr_fn::ident;
use datafusion::prelude::lit;
use indicatif::ProgressBar;

use super::execution::collect_dataframe_with_metric_polling;
use super::plan::RangePartitionBoundaries;

/// Estimate balanced spatial-code ranges with one minimum and approximate percentiles.
pub(crate) async fn compute_range_partition_boundaries(
  dataframe: engine::DataFrame,
  sort_column: &str,
  bucket_count: usize,
  progress_bar: &ProgressBar,
  total_input_rows: u64,
  explain: bool,
) -> Result<RangePartitionBoundaries> {
  if bucket_count <= 1 {
    return Ok(RangePartitionBoundaries {
      min_value: 0,
      boundaries: Vec::new(),
    });
  }

  let mut aggregate_expressions = vec![min(ident(sort_column)).alias("range_min")];
  aggregate_expressions.extend((1..bucket_count).map(|index| {
    approx_percentile_cont(
      ident(sort_column).sort(true, false),
      lit(index as f64 / bucket_count as f64),
      None,
    )
    .alias(format!("range_boundary_{index}"))
  }));
  let batches = collect_dataframe_with_metric_polling(
    dataframe.aggregate(vec![], aggregate_expressions)?,
    progress_bar,
    total_input_rows,
    "Computing partition ranges",
    explain,
  )
  .await?;
  let Some(batch) = batches.first() else {
    return Ok(RangePartitionBoundaries {
      min_value: 0,
      boundaries: Vec::new(),
    });
  };
  if batch.num_rows() == 0 {
    return Ok(RangePartitionBoundaries {
      min_value: 0,
      boundaries: Vec::new(),
    });
  }

  let minimum_values = batch
    .column(0)
    .as_any()
    .downcast_ref::<UInt64Array>()
    .context("range partition minimum aggregate did not return UInt64")?;
  let min_value = if minimum_values.is_null(0) {
    0
  } else {
    minimum_values.value(0)
  };

  let mut boundaries = Vec::with_capacity(batch.num_columns().saturating_sub(1));
  for column in batch.columns().iter().skip(1) {
    let values = column
      .as_any()
      .downcast_ref::<UInt64Array>()
      .context("range boundary aggregate did not return UInt64")?;
    if !values.is_null(0) {
      boundaries.push(values.value(0));
    }
  }
  boundaries.sort_unstable();
  Ok(RangePartitionBoundaries {
    min_value,
    boundaries,
  })
}

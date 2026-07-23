//! Computes balanced cluster-key ranges for exact multi-file output.

use anyhow::Result;
use arrow_array::{Array, ArrayRef, Float64Array, UInt64Array};
use datafusion::dataframe::DataFrame;
use datafusion::functions_aggregate::approx_percentile_cont::approx_percentile_cont;
use datafusion::functions_aggregate::expr_fn::min;
use datafusion::logical_expr::expr_fn::ident;
use datafusion::prelude::lit;

use crate::diagnostics::Diagnostics;

use crate::optimized::ClusterRangeBoundaries;

impl ClusterRangeBoundaries {
  /// Compute balanced cluster-key ranges from one dataframe and target bucket count.
  pub(super) async fn compute(
    dataframe: DataFrame,
    cluster_key_column: &str,
    bucket_count: usize,
  ) -> Result<Self> {
    if bucket_count <= 1 {
      return Ok(Self::new(0, Vec::new()));
    }

    let mut aggregate_expressions = vec![min(ident(cluster_key_column)).alias("range_min")];
    aggregate_expressions.extend((1..bucket_count).map(|index| {
      approx_percentile_cont(
        ident(cluster_key_column).sort(true, false),
        lit(index as f64 / bucket_count as f64),
        None,
      )
      .alias(format!("range_boundary_{index}"))
    }));
    let aggregate_dataframe = dataframe.aggregate(vec![], aggregate_expressions)?;
    let batches = Diagnostics::with("cluster boundary aggregate")
      .collect(aggregate_dataframe)
      .await?;
    let Some(batch) = batches.first() else {
      return Ok(Self::new(0, Vec::new()));
    };
    if batch.num_rows() == 0 {
      return Ok(Self::new(0, Vec::new()));
    }

    let min_value = aggregate_u64(batch.column(0))?.unwrap_or(0);

    let mut boundaries = Vec::with_capacity(batch.num_columns().saturating_sub(1));
    for column in batch.columns().iter().skip(1) {
      if let Some(value) = aggregate_u64(column)? {
        boundaries.push(value);
      }
    }
    boundaries.sort_unstable();
    Ok(Self::new(min_value, boundaries))
  }
}

fn aggregate_u64(values: &ArrayRef) -> Result<Option<u64>> {
  if values.is_null(0) {
    return Ok(None);
  }
  if let Some(values) = values.as_any().downcast_ref::<UInt64Array>() {
    return Ok(Some(values.value(0)));
  }
  if let Some(values) = values.as_any().downcast_ref::<Float64Array>() {
    return Ok(Some(values.value(0) as u64));
  }
  anyhow::bail!(
    "range aggregate returned unsupported type {}",
    values.data_type()
  )
}

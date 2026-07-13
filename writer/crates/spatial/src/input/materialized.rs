//! Materializes bounded input ranges shared by independent DataFusion plans.

use anyhow::{Result, bail};
use arrow_array::RecordBatch;
use futures_util::StreamExt;
use indicatif::ProgressBar;
use std::time::Instant;

use super::{InputSource, RowRange, is_http_url};
use crate::diagnostics::explain_timing;
use crate::progress::{finish_row_bar, row_bar};

pub(crate) const MAX_HTTP_RANGE_ROWS: usize = 100;

/// Return whether a bounded HTTP range should be cached before repeated scans.
pub(crate) fn should_materialize_bounded_http_range(
  input: &dyn InputSource,
  row_range: RowRange,
) -> bool {
  matches!(row_range.num, Some(num) if num <= MAX_HTTP_RANGE_ROWS)
    && !row_range.is_full()
    && is_http_url(input.source_location())
}

/// Validate bounded-read requirements for direct HTTP Parquet input.
pub(crate) fn validate_http_row_range(input_location: &str, row_range: RowRange) -> Result<()> {
  if !is_http_url(input_location) {
    return Ok(());
  }
  if row_range.start > 0 && row_range.num.is_none() {
    bail!("HTTP parquet input with --start requires --num (maximum {MAX_HTTP_RANGE_ROWS} rows)");
  }
  if let Some(num) = row_range.num
    && num > MAX_HTTP_RANGE_ROWS
  {
    bail!("HTTP parquet input supports at most --num {MAX_HTTP_RANGE_ROWS} for ranged reads");
  }
  Ok(())
}

/// Materialize a selected row range once to avoid repeated source scans.
pub(crate) async fn materialize_input_row_range(
  input: &dyn InputSource,
  row_range: RowRange,
  progress_bar: &ProgressBar,
) -> Result<Vec<RecordBatch>> {
  let mut batches = Vec::new();
  let mut stream = input.read_batches(row_range).await?;
  while let Some(batch) = stream.next().await {
    let batch = batch?;
    progress_bar.inc(batch.num_rows() as u64);
    batches.push(batch);
  }
  if batches.is_empty() {
    batches.push(RecordBatch::new_empty(input.schema()?));
  }
  Ok(batches)
}

/// Build a source DataFrame or reuse previously materialized batches.
pub(crate) async fn input_dataframe_for_job(
  input: &dyn InputSource,
  context: &engine::SessionContext,
  row_range: RowRange,
  materialized_batches: Option<&[RecordBatch]>,
) -> Result<engine::DataFrame> {
  if let Some(batches) = materialized_batches {
    return Ok(context.read_batches(batches.iter().cloned())?);
  }
  input.to_dataframe(context, row_range).await
}

/// Materialize a selected HTTP range and report its workflow progress.
pub(crate) async fn materialize_selected_http_range(
  input: &dyn InputSource,
  row_range: RowRange,
  total_input_rows: u64,
  progress: bool,
  explain: bool,
) -> Result<Option<Vec<RecordBatch>>> {
  if !should_materialize_bounded_http_range(input, row_range) {
    return Ok(None);
  }

  let range_bar = row_bar(progress, "Reading selected row range", total_input_rows);
  let range_start = Instant::now();
  let batches = materialize_input_row_range(input, row_range, &range_bar).await?;
  finish_row_bar(
    &range_bar,
    total_input_rows,
    "Read selected row range".to_string(),
  );
  explain_timing(explain, "Reading selected row range", range_start.elapsed());
  Ok(Some(batches))
}

//! Collects aggregate DataFrames while polling physical-plan metrics for progress.

use std::sync::{
  Arc,
  atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use anyhow::Result;
use arrow_array::RecordBatch;
use datafusion::execution::TaskContext;
use datafusion::physical_plan::collect;
use indicatif::ProgressBar;

use crate::diagnostics::{
  explain_dataframe_verbose, explain_physical_plan, explain_stage_completion,
};
use crate::progress::{collect_plan_progress, update_metric_count_bar};

/// Execute an aggregate DataFrame while polling physical-plan metrics.
pub(crate) async fn collect_aggregate_with_progress(
  dataframe: engine::DataFrame,
  progress_bar: &ProgressBar,
  total_input_rows: u64,
  base_message: &str,
  explain: bool,
) -> Result<Vec<RecordBatch>> {
  let (state, logical_plan) = dataframe.into_parts();
  explain_dataframe_verbose(explain, base_message, &state, &logical_plan, true).await?;
  let physical_plan = state.create_physical_plan(&logical_plan).await?;
  explain_physical_plan(explain, base_message, &physical_plan);
  let task_context = Arc::new(TaskContext::from(&state));
  let stage_start = Instant::now();

  let done = Arc::new(AtomicBool::new(false));
  let poller = if progress_bar.is_hidden() {
    None
  } else {
    let plan = Arc::clone(&physical_plan);
    let bar = progress_bar.clone();
    let done = Arc::clone(&done);
    let base_message = base_message.to_string();
    Some(std::thread::spawn(move || {
      while !done.load(Ordering::Relaxed) {
        update_metric_count_bar(
          &bar,
          &base_message,
          "finalizing aggregates",
          collect_plan_progress(plan.as_ref()),
          total_input_rows,
        );
        std::thread::sleep(Duration::from_millis(500));
      }
    }))
  };

  let result = collect(Arc::clone(&physical_plan), task_context).await;
  done.store(true, Ordering::Relaxed);
  if let Some(poller) = poller {
    let _ = poller.join();
  }

  explain_stage_completion(
    explain,
    base_message,
    stage_start.elapsed(),
    &physical_plan,
    collect_plan_progress(physical_plan.as_ref()),
  );
  result.map_err(Into::into)
}

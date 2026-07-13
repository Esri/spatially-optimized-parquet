//! Reports optional DataFusion plans, runtime metrics, spills, timings, and operator hotspots.
//!
//! Diagnostics remain separate from progress presentation. Explain mode can inspect logical and
//! physical plans, while completed stages report measured operator activity without changing the
//! DataFusion execution path.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use datafusion::common::format::{ExplainAnalyzeLevel, ExplainFormat};
use datafusion::execution::context::SessionState;
use datafusion::logical_expr::{ExplainOption, LogicalPlan};
use datafusion::physical_plan::{
  ExecutionPlan, ExecutionPlanProperties, displayable, metrics::MetricType,
};

use crate::progress::{PlanProgressMetrics, SINK_ROWS_METRIC, format_bytes, format_elapsed_debug};

pub(crate) fn explain_stage_note(explain: bool, stage: &str, note: &str) {
  if explain {
    eprintln!("[explain] {stage}: {note}");
  }
}

pub(crate) fn configure_explain_session(ctx: &engine::SessionContext, explain: bool) {
  if !explain {
    return;
  }
  let state_ref = ctx.state_ref();
  let mut state = state_ref.write();
  let options = &mut state.config_mut().options_mut().explain;
  options.logical_plan_only = false;
  options.physical_plan_only = false;
  options.show_statistics = true;
  options.show_schema = false;
  options.show_sizes = true;
  options.format = ExplainFormat::Indent;
  options.analyze_level = ExplainAnalyzeLevel::Dev;
}

pub(crate) async fn explain_dataframe_verbose(
  explain: bool,
  stage: &str,
  state: &SessionState,
  logical_plan: &LogicalPlan,
  run_analyze_verbose: bool,
) -> Result<()> {
  if !explain {
    return Ok(());
  }
  let verbose = engine::DataFrame::new(state.clone(), logical_plan.clone())
    .explain_with_options(
      ExplainOption::default()
        .with_verbose(true)
        .with_analyze(false)
        .with_format(ExplainFormat::Indent),
    )?
    .to_string()
    .await?;
  eprintln!("[explain] {stage} DataFusion EXPLAIN VERBOSE:");
  eprintln!("{verbose}");
  if run_analyze_verbose {
    let analyzed = engine::DataFrame::new(state.clone(), logical_plan.clone())
      .explain_with_options(
        ExplainOption::default()
          .with_verbose(true)
          .with_analyze(true)
          .with_format(ExplainFormat::Indent),
      )?
      .to_string()
      .await?;
    eprintln!("[explain] {stage} DataFusion EXPLAIN ANALYZE VERBOSE:");
    eprintln!("{analyzed}");
  } else {
    eprintln!(
      "[explain] {stage}: skipping DataFusion EXPLAIN ANALYZE VERBOSE re-execution because this stage has write side effects; see the post-run physical-plan metrics below"
    );
  }
  Ok(())
}

pub(crate) fn explain_physical_plan(explain: bool, stage: &str, plan: &Arc<dyn ExecutionPlan>) {
  if !explain {
    return;
  }
  eprintln!(
    "[explain] {stage} physical plan (output_partitions={}):",
    plan.output_partitioning().partition_count()
  );
  eprintln!(
    "{}",
    displayable(plan.as_ref())
      .set_show_statistics(true)
      .indent(true)
  );
}

pub(crate) fn explain_stage_completion(
  explain: bool,
  stage: &str,
  elapsed: Duration,
  plan: &Arc<dyn ExecutionPlan>,
  metrics: PlanProgressMetrics,
) {
  if !explain {
    return;
  }
  eprintln!("[timing] {stage}: {}", format_elapsed_debug(elapsed));
  eprintln!(
    "[metrics] {stage}: rows_read={} rows_written={} spill_count={} spilled={} compute={}",
    metrics.rows_read,
    metrics.rows_written,
    metrics.spill_count,
    format_bytes(metrics.spilled_bytes),
    format_elapsed_debug(Duration::from_nanos(metrics.elapsed_compute_nanos)),
  );
  explain_physical_plan_with_metrics(stage, plan);
  explain_operator_hotspots(stage, plan);
}

pub(crate) fn explain_timing(explain: bool, label: &str, elapsed: Duration) {
  if explain {
    eprintln!("[timing] {label}: {}", format_elapsed_debug(elapsed));
  }
}

fn explain_physical_plan_with_metrics(stage: &str, plan: &Arc<dyn ExecutionPlan>) {
  let metric_types = vec![MetricType::SUMMARY, MetricType::DEV];
  eprintln!("[explain] {stage} physical plan with metrics (DataFusion EXPLAIN ANALYZE):");
  eprintln!(
    "{}",
    datafusion::physical_plan::display::DisplayableExecutionPlan::with_metrics(plan.as_ref())
      .set_show_statistics(true)
      .set_metric_types(metric_types.clone())
      .indent(true)
  );
  eprintln!(
    "[explain] {stage} physical plan with full metrics (DataFusion EXPLAIN ANALYZE VERBOSE):"
  );
  eprintln!(
    "{}",
    datafusion::physical_plan::display::DisplayableExecutionPlan::with_full_metrics(plan.as_ref())
      .set_show_statistics(true)
      .set_metric_types(metric_types)
      .indent(true)
  );
}

#[derive(Clone, Debug)]
struct OperatorMetricSnapshot {
  operator: String,
  depth: usize,
  output_partitions: usize,
  output_rows: u64,
  rows_written: u64,
  spill_count: u64,
  spilled_bytes: u64,
  elapsed_compute_nanos: u64,
}

fn explain_operator_hotspots(stage: &str, plan: &Arc<dyn ExecutionPlan>) {
  let mut snapshots = Vec::new();
  collect_operator_metric_snapshots(plan.as_ref(), 0, &mut snapshots);
  snapshots.retain(|snapshot| {
    snapshot.elapsed_compute_nanos > 0
      || snapshot.spill_count > 0
      || snapshot.spilled_bytes > 0
      || snapshot.rows_written > 0
      || snapshot.output_rows > 0
  });
  snapshots.sort_by(|left, right| {
    right
      .elapsed_compute_nanos
      .cmp(&left.elapsed_compute_nanos)
      .then(right.spilled_bytes.cmp(&left.spilled_bytes))
      .then(right.spill_count.cmp(&left.spill_count))
      .then(right.rows_written.cmp(&left.rows_written))
      .then(right.output_rows.cmp(&left.output_rows))
      .then(left.depth.cmp(&right.depth))
  });
  if snapshots.is_empty() {
    return;
  }
  eprintln!("[operator-metrics] {stage} hottest operators by compute:");
  for (index, snapshot) in snapshots.into_iter().take(12).enumerate() {
    eprintln!(
      "  {:>2}. operator={} depth={} out_partitions={} output_rows={} sink_rows={} compute={} spill_count={} spilled={}",
      index + 1,
      snapshot.operator,
      snapshot.depth,
      snapshot.output_partitions,
      snapshot.output_rows,
      snapshot.rows_written,
      format_elapsed_debug(Duration::from_nanos(snapshot.elapsed_compute_nanos)),
      snapshot.spill_count,
      format_bytes(snapshot.spilled_bytes),
    );
  }
}

fn collect_operator_metric_snapshots(
  plan: &dyn ExecutionPlan,
  depth: usize,
  snapshots: &mut Vec<OperatorMetricSnapshot>,
) {
  let mut snapshot = OperatorMetricSnapshot {
    operator: plan.name().to_string(),
    depth,
    output_partitions: plan.output_partitioning().partition_count(),
    output_rows: 0,
    rows_written: 0,
    spill_count: 0,
    spilled_bytes: 0,
    elapsed_compute_nanos: 0,
  };
  if let Some(metrics) = plan.metrics() {
    snapshot.output_rows = metrics.output_rows().unwrap_or(0) as u64;
    snapshot.rows_written = metrics
      .sum_by_name(SINK_ROWS_METRIC)
      .map(|value| value.as_usize() as u64)
      .unwrap_or(0);
    snapshot.spill_count = metrics.spill_count().unwrap_or(0) as u64;
    snapshot.spilled_bytes = metrics.spilled_bytes().unwrap_or(0) as u64;
    snapshot.elapsed_compute_nanos = metrics.elapsed_compute().unwrap_or(0) as u64;
  }
  snapshots.push(snapshot);
  for child in plan.children() {
    collect_operator_metric_snapshots(child.as_ref(), depth + 1, snapshots);
  }
}

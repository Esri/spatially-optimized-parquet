//! Presents job-wide progress independently from source, GeoParquet, and writer semantics.
//!
//! DataFusion exposes physical-plan metrics rather than workflow phases. This module aggregates
//! those metrics, derives read/sort/write phases, and renders consistent row, spill, and compute
//! status for input materialization, analysis, plain output, and optimized output.

use std::io::IsTerminal;
use std::time::Duration;

use datafusion::physical_plan::ExecutionPlan;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

pub(crate) const SINK_ROWS_METRIC: &str = "sink_rows";

/// Aggregates row, spill, and compute metrics across a physical-plan tree.
#[derive(Default, Clone, Copy)]
pub(crate) struct PlanProgressMetrics {
  pub(crate) rows_read: u64,
  pub(crate) rows_written: u64,
  pub(crate) spill_count: u64,
  pub(crate) spilled_bytes: u64,
  pub(crate) elapsed_compute_nanos: u64,
}

/// Identifies the observable phase of a final write plan.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum WriteStagePhase {
  Reading,
  Sorting,
  Writing,
}

/// Collect progress metrics from a complete physical-plan tree.
pub(crate) fn collect_plan_progress(plan: &dyn ExecutionPlan) -> PlanProgressMetrics {
  let mut metrics = PlanProgressMetrics::default();
  accumulate_plan_progress(plan, &mut metrics);
  metrics
}

fn accumulate_plan_progress(plan: &dyn ExecutionPlan, metrics: &mut PlanProgressMetrics) -> bool {
  let mut child_has_row_metric = false;
  for child in plan.children() {
    child_has_row_metric |= accumulate_plan_progress(child.as_ref(), metrics);
  }
  let mut plan_has_row_metric = false;
  if let Some(plan_metrics) = plan.metrics() {
    metrics.spill_count += plan_metrics.spill_count().unwrap_or(0) as u64;
    metrics.spilled_bytes += plan_metrics.spilled_bytes().unwrap_or(0) as u64;
    metrics.elapsed_compute_nanos += plan_metrics.elapsed_compute().unwrap_or(0) as u64;
    if let Some(rows_written) = plan_metrics.sum_by_name(SINK_ROWS_METRIC) {
      metrics.rows_written += rows_written.as_usize() as u64;
    }
    if let Some(output_rows) = plan_metrics.output_rows() {
      plan_has_row_metric = true;
      if !child_has_row_metric {
        metrics.rows_read += output_rows as u64;
      }
    }
  }
  child_has_row_metric || plan_has_row_metric
}

/// Update aggregate-stage progress from physical-plan metrics.
pub(crate) fn update_metric_count_bar(
  progress_bar: &ProgressBar,
  base_message: &str,
  post_read_message: &str,
  metrics: PlanProgressMetrics,
  total_input_rows: u64,
) {
  if progress_bar.is_hidden() {
    return;
  }
  if total_input_rows > 0 {
    progress_bar.set_position(metrics.rows_read.min(total_input_rows));
  }
  progress_bar.set_message(format_metric_progress_message(
    base_message,
    post_read_message,
    metrics,
    total_input_rows,
    false,
  ));
}

/// Derive and render the read, sort, or write phase of a final write plan.
pub(crate) fn update_write_stage_bar(
  progress_bar: &ProgressBar,
  metrics: PlanProgressMetrics,
  total_input_rows: u64,
  partitioned_write: bool,
  current_phase: &mut Option<WriteStagePhase>,
) {
  if progress_bar.is_hidden() {
    return;
  }
  let phase = if metrics.rows_written > 0 {
    WriteStagePhase::Writing
  } else if total_input_rows > 0 && metrics.rows_read >= total_input_rows {
    WriteStagePhase::Sorting
  } else {
    WriteStagePhase::Reading
  };
  if current_phase != &Some(phase) {
    match phase {
      WriteStagePhase::Reading | WriteStagePhase::Writing => {
        progress_bar.set_style(count_bar_style("rows"));
        progress_bar.set_length(total_input_rows.max(1));
      }
      WriteStagePhase::Sorting => progress_bar.set_style(message_only_style()),
    }
    *current_phase = Some(phase);
  }
  progress_bar.set_message(write_stage_message(phase, partitioned_write).to_string());
  match phase {
    WriteStagePhase::Reading => {
      progress_bar.set_position(metrics.rows_read.min(total_input_rows));
    }
    WriteStagePhase::Sorting => {}
    WriteStagePhase::Writing => {
      progress_bar.set_position(metrics.rows_written.min(total_input_rows));
    }
  }
}

/// Return the user-facing label for one output phase and layout.
pub(crate) fn write_stage_message(phase: WriteStagePhase, partitioned_write: bool) -> &'static str {
  match (phase, partitioned_write) {
    (WriteStagePhase::Reading, false) => "Preparing output — reading rows into sort buffers",
    (WriteStagePhase::Sorting, false) => "Preparing output — sorting buffered rows",
    (WriteStagePhase::Writing, false) => "Writing parquet file",
    (WriteStagePhase::Reading, true) => {
      "Preparing output — reading rows into sort buffers for multiple files"
    }
    (WriteStagePhase::Sorting, true) => "Preparing output — sorting rows for multiple files",
    (WriteStagePhase::Writing, true) => "Writing parquet files",
  }
}

pub(crate) fn row_bar(enabled: bool, message: &str, total_rows: u64) -> ProgressBar {
  count_bar(enabled, message, total_rows, "rows")
}

pub(crate) fn count_bar(enabled: bool, message: &str, total: u64, unit: &str) -> ProgressBar {
  if !enabled || !std::io::stderr().is_terminal() {
    return ProgressBar::hidden();
  }
  count_bar_with_parent(message, total, unit, None)
}

pub(crate) fn count_bar_with_parent(
  message: &str,
  total: u64,
  unit: &str,
  parent: Option<&MultiProgress>,
) -> ProgressBar {
  let bar = ProgressBar::new(total.max(1));
  bar.set_style(count_bar_style(unit));
  bar.set_message(message.to_string());
  if let Some(parent) = parent {
    parent.add(bar)
  } else {
    bar
  }
}

pub(crate) fn finish_spinner(bar: &ProgressBar, message: String) {
  if bar.is_hidden() {
    return;
  }
  bar.set_style(message_only_style());
  bar.finish_with_message(format!("{message} in {}", format_elapsed(bar.elapsed())));
}

pub(crate) fn finish_row_bar(bar: &ProgressBar, total_rows: u64, message: String) {
  finish_count_bar(bar, total_rows, message);
}

pub(crate) fn finish_count_bar(bar: &ProgressBar, total: u64, message: String) {
  if bar.is_hidden() {
    return;
  }
  bar.set_position(total.max(1));
  bar.set_style(message_only_style());
  bar.finish_with_message(format!("{message} in {}", format_elapsed(bar.elapsed())));
}

pub(crate) fn format_elapsed(duration: Duration) -> String {
  let total_seconds = duration.as_secs();
  let hours = total_seconds / 3600;
  let minutes = (total_seconds % 3600) / 60;
  let seconds = total_seconds % 60;
  if hours > 0 {
    format!("{hours:02}:{minutes:02}:{seconds:02}")
  } else {
    format!("{minutes:02}:{seconds:02}")
  }
}

pub(crate) fn format_elapsed_debug(duration: Duration) -> String {
  if duration >= Duration::from_secs(1) {
    return format_elapsed(duration);
  }
  if duration >= Duration::from_millis(1) {
    return format!("{:.2}ms", duration.as_secs_f64() * 1_000.0);
  }
  if duration >= Duration::from_micros(1) {
    return format!("{:.2}µs", duration.as_secs_f64() * 1_000_000.0);
  }
  format!("{}ns", duration.as_nanos())
}

pub(crate) fn format_bytes(bytes: u64) -> String {
  const KIB: u64 = 1024;
  const MIB: u64 = 1024 * KIB;
  const GIB: u64 = 1024 * MIB;
  if bytes >= GIB {
    format!("{:.1} GiB", bytes as f64 / GIB as f64)
  } else if bytes >= MIB {
    format!("{:.1} MiB", bytes as f64 / MIB as f64)
  } else if bytes >= KIB {
    format!("{:.1} KiB", bytes as f64 / KIB as f64)
  } else {
    format!("{bytes} B")
  }
}

fn count_bar_style(unit: &str) -> ProgressStyle {
  ProgressStyle::with_template(&format!(
    "{{msg:20}} [{{bar:40.cyan/blue}}] {{pos}}/{{len}} {unit} ({{percent}}%)"
  ))
  .expect("count progress template should be valid")
  .progress_chars("=>-")
}

fn message_only_style() -> ProgressStyle {
  ProgressStyle::with_template("{msg}").expect("message progress template should be valid")
}

pub(crate) fn format_metric_progress_message(
  base_message: &str,
  post_read_message: &str,
  metrics: PlanProgressMetrics,
  total_input_rows: u64,
  allow_sink_rows: bool,
) -> String {
  if allow_sink_rows && metrics.rows_written > 0 {
    let rows_written = if total_input_rows > 0 {
      metrics.rows_written.min(total_input_rows)
    } else {
      metrics.rows_written
    };
    return if total_input_rows > 0 {
      format!(
        "{base_message} — writing {rows_written}/{total_input_rows} rows ({}%)",
        format_progress_percent(rows_written, total_input_rows)
      )
    } else {
      format!("{base_message} — writing {rows_written} rows")
    };
  }
  let mut message = if metrics.rows_read == 0 {
    format!("{base_message} — scanning source")
  } else if total_input_rows > 0 && metrics.rows_read < total_input_rows {
    base_message.to_string()
  } else if total_input_rows > 0 {
    format!(
      "{base_message} — {}",
      format_post_read_activity(post_read_message, metrics)
    )
  } else {
    base_message.to_string()
  };
  if (total_input_rows == 0 || metrics.rows_read < total_input_rows)
    && (metrics.spill_count > 0 || metrics.spilled_bytes > 0)
  {
    message.push_str(&format!(
      " — spills {} ({})",
      metrics.spill_count,
      format_bytes(metrics.spilled_bytes)
    ));
  }
  message
}

fn format_post_read_activity(post_read_message: &str, metrics: PlanProgressMetrics) -> String {
  let mut message = if metrics.spill_count > 0 || metrics.spilled_bytes > 0 {
    format!(
      "{post_read_message} — spills {} ({})",
      metrics.spill_count,
      format_bytes(metrics.spilled_bytes)
    )
  } else {
    format!("{post_read_message} in memory")
  };
  if metrics.elapsed_compute_nanos > 0 {
    message.push_str(&format!(
      " — compute {}",
      format_elapsed(Duration::from_nanos(metrics.elapsed_compute_nanos))
    ));
  }
  message
}

fn format_progress_percent(rows: u64, total_rows: u64) -> u64 {
  if total_rows == 0 {
    return 0;
  }
  (((rows as u128) * 100) / total_rows as u128) as u64
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn formats_scan_progress_message() {
    assert_eq!(
      format_metric_progress_message(
        "Encoding display payload and writing parquet",
        "sorting buffered rows",
        PlanProgressMetrics {
          rows_read: 128,
          ..Default::default()
        },
        1024,
        true,
      ),
      "Encoding display payload and writing parquet"
    );
  }

  #[test]
  fn formats_post_read_activity_message() {
    assert_eq!(
      format_metric_progress_message(
        "Encoding display payload and writing parquet",
        "sorting buffered rows",
        PlanProgressMetrics {
          rows_read: 1024,
          elapsed_compute_nanos: Duration::from_secs(12).as_nanos() as u64,
          ..Default::default()
        },
        1024,
        true,
      ),
      "Encoding display payload and writing parquet — sorting buffered rows in memory — compute 00:12"
    );
  }

  #[test]
  fn formats_sink_side_progress_message() {
    assert_eq!(
      format_metric_progress_message(
        "Encoding display payload and writing parquet",
        "sorting buffered rows",
        PlanProgressMetrics {
          rows_read: 1024,
          rows_written: 256,
          ..Default::default()
        },
        1024,
        true,
      ),
      "Encoding display payload and writing parquet — writing 256/1024 rows (25%)"
    );
  }

  #[test]
  fn groups_write_stage_labels_by_output_layout() {
    assert_eq!(
      write_stage_message(WriteStagePhase::Reading, false),
      "Preparing output — reading rows into sort buffers"
    );
    assert_eq!(
      write_stage_message(WriteStagePhase::Writing, false),
      "Writing parquet file"
    );
    assert_eq!(
      write_stage_message(WriteStagePhase::Reading, true),
      "Preparing output — reading rows into sort buffers for multiple files"
    );
    assert_eq!(
      write_stage_message(WriteStagePhase::Writing, true),
      "Writing parquet files"
    );
  }
}

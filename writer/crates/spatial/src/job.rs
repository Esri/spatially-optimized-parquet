//! Orchestrates source opening and selects plain or optimized GeoParquet output.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use arrow_schema::SchemaRef;
use engine::plan::{OutputPlan, validate_output};
use engine::session::{DataFusionSession, new_datafusion_session};

use crate::diagnostics::{configure_explain_session, explain_stage_note, explain_timing};
use crate::input::materialized::{materialize_selected_http_range, validate_http_row_range};
use crate::input::{
  InputOpenOptions, InputSource, RowRange, SourceFormat, open_input, resolve_source_format,
};
use crate::output::GeoParquetOutputMode;
use crate::output::geoparquet::validate_covering_configuration;
use crate::output::optimized::{OptimizeOutputRequest, run as run_optimized_output};
use crate::output::plain::{PlainOutputRequest, write as write_plain_geoparquet};
use crate::progress::format_elapsed;
use crate::udf::register_display_udfs;

/// Configures one complete optimization or pass-through execution.
pub struct OptimizeJobOptions {
  /// Stores the local path or HTTP URL to read.
  pub input: String,
  /// Overrides source-format inference for extensionless or unconventional locations.
  pub input_format: Option<SourceFormat>,
  /// Stores the output file or directory.
  pub output: PathBuf,
  /// Selects the number of generated files for directory output.
  pub output_files: Option<usize>,
  /// Selects the Parquet compression codec by name.
  pub compression: Option<String>,
  /// Selects the contiguous input row range.
  pub row_range: RowRange,
  /// Selects one layer from a multi-layer input.
  pub layer: Option<String>,
  /// Overrides geometry-column inference.
  pub geometry_column: Option<String>,
  /// Supplies an input EPSG code only when source CRS metadata is absent.
  pub input_wkid: Option<u32>,
  /// Enables a GeoParquet 1.1 covering bbox column.
  pub covering: bool,
  /// Allows replacement of a compatible existing output.
  pub overwrite: bool,
  /// Enables interactive progress reporting.
  pub progress: bool,
  /// Enables verbose DataFusion plans, metrics, and timing diagnostics.
  pub explain: bool,
  /// Selects plain or Spatially Optimized GeoParquet output.
  pub output_mode: GeoParquetOutputMode,
}

/// Owns validated source, destination, and execution resources for one job.
struct OpenedOptimizeJob {
  input: Arc<dyn InputSource>,
  output_plan: OutputPlan,
  source_schema: SchemaRef,
  total_input_rows: u64,
  session: DataFusionSession,
}

/// Execute one spatial optimization job from provider selection through durable output.
pub async fn run_optimize_job(options: OptimizeJobOptions) -> Result<()> {
  let job_start = Instant::now();
  let job = open_optimize_job(&options).await?;
  let materialized_row_range = materialize_selected_http_range(
    job.input.as_ref(),
    options.row_range,
    job.total_input_rows,
    options.progress,
    options.explain,
  )
  .await?;

  if options.output_mode == GeoParquetOutputMode::Plain {
    let rows_written = write_plain_geoparquet(PlainOutputRequest {
      input: job.input.as_ref(),
      session: job.session.context(),
      output_plan: &job.output_plan,
      row_range: options.row_range,
      materialized_batches: materialized_row_range.as_deref(),
      geometry_column: options.geometry_column.as_deref(),
      input_wkid: options.input_wkid,
      covering: options.covering,
      compression: options.compression.as_deref(),
    })
    .await?;
    explain_stage_note(
      options.explain,
      "Plain GeoParquet",
      &format!("wrote {rows_written} selected rows without SOP display optimization"),
    );
  } else {
    run_optimized_output(OptimizeOutputRequest {
      input: job.input.as_ref(),
      session: job.session.context(),
      output_plan: &job.output_plan,
      source_schema: job.source_schema.as_ref(),
      total_input_rows: job.total_input_rows,
      row_range: options.row_range,
      materialized_batches: materialized_row_range.as_deref(),
      geometry_column: options.geometry_column.as_deref(),
      input_wkid: options.input_wkid,
      covering: options.covering,
      compression: options.compression.as_deref(),
      progress: options.progress,
      explain: options.explain,
    })
    .await?;
  }

  report_job_completion(&options, job_start);
  Ok(())
}

/// Open and validate source, destination, and DataFusion resources for one job.
async fn open_optimize_job(options: &OptimizeJobOptions) -> Result<OpenedOptimizeJob> {
  validate_http_row_range(&options.input, options.row_range)?;
  let input_format = resolve_source_format(&options.input, options.input_format)?;
  let input = open_input(
    input_format,
    &InputOpenOptions {
      location: options.input.clone(),
      layer: options.layer.clone(),
    },
  )
  .await?;
  let output_plan = validate_output(&options.output, options.output_files, options.overwrite)?;
  let source_schema = input.schema()?;
  validate_covering_configuration(options.covering, source_schema.as_ref())?;
  let discovered_rows = input.total_rows()?;
  let total_input_rows = options.row_range.effective_rows(discovered_rows);
  explain_run_configuration(
    options.explain,
    input.format_name(),
    options,
    discovered_rows,
    total_input_rows,
    output_plan.parts,
  );

  let session = new_datafusion_session()?;
  configure_explain_session(session.context(), options.explain);
  register_display_udfs(session.context());
  Ok(OpenedOptimizeJob {
    input,
    output_plan,
    source_schema,
    total_input_rows,
    session,
  })
}

fn report_job_completion(options: &OptimizeJobOptions, job_start: Instant) {
  if options.progress && std::io::stderr().is_terminal() {
    eprintln!("Completed in {}", format_elapsed(job_start.elapsed()));
  }
  explain_timing(options.explain, "Total job", job_start.elapsed());
}

fn explain_run_configuration(
  explain: bool,
  input_format: &str,
  options: &OptimizeJobOptions,
  discovered_rows: u64,
  effective_rows: u64,
  output_parts: usize,
) {
  if !explain {
    return;
  }
  eprintln!(
    "[explain] input_format={input_format} discovered_rows={discovered_rows} effective_rows={effective_rows} output_files={output_parts} start={} num={}",
    options.row_range.start,
    options
      .row_range
      .num
      .map(|num| num.to_string())
      .unwrap_or_else(|| "all".to_string())
  );
  eprintln!(
    "[explain] output_path={} overwrite={} progress={}",
    options.output.display(),
    options.overwrite,
    options.progress,
  );
}

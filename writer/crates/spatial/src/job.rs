//! Orchestrates source opening and selects plain or optimized GeoParquet output.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use arrow_schema::SchemaRef;
use engine::output_layout::{OutputLayout, resolve_output_layout};
use engine::session::{DataFusionSession, new_datafusion_session};

use crate::diagnostics::{configure_explain_session, explain_stage_note, explain_timing};
use crate::geoparquet::{PlainGeoParquet, validate_covering_configuration};
use crate::input::{
  InputOpenOptions, InputSource, RowRange, SourceFormat, open_input, resolve_source_format,
};
use crate::optimized::OptimizedGeoParquet;
use crate::optimized::multiscale::validate_internal_projection_columns;
use crate::output::stage::{OutputStage, OutputStageContext};
use crate::output::{GeoParquetOutputMode, validate_output_wkid};
use crate::progress::{finish_row_bar, format_elapsed, row_bar};

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
  /// Selects the output spatial reference.
  pub output_wkid: u32,
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
  _session: DataFusionSession,
  input: Arc<dyn InputSource>,
  output_layout: OutputLayout,
  source_schema: SchemaRef,
  total_input_rows: u64,
  input_dataframe: engine::DataFrame,
}

/// Execute one spatial optimization job from provider selection through durable output.
pub async fn run_optimize_job(options: OptimizeJobOptions) -> Result<()> {
  validate_output_wkid(options.output_wkid);
  let job_start = Instant::now();
  let job = open_optimize_job(&options).await?;

  let output_context = OutputStageContext {
    input: job.input.as_ref(),
    input_dataframe: job.input_dataframe.clone(),
    output_layout: &job.output_layout,
    source_schema: job.source_schema.as_ref(),
    total_input_rows: job.total_input_rows,
    row_range: options.row_range,
    geometry_column: options.geometry_column.as_deref(),
    input_wkid: options.input_wkid,
    output_wkid: options.output_wkid,
    covering: options.covering,
    compression: options.compression.as_deref(),
    progress: options.progress,
    explain: options.explain,
  };
  let output_result = match options.output_mode {
    GeoParquetOutputMode::Plain => PlainGeoParquet.execute(output_context).await?,
    GeoParquetOutputMode::Optimized => OptimizedGeoParquet.execute(output_context).await?,
  };

  if options.output_mode == GeoParquetOutputMode::Plain {
    explain_stage_note(
      options.explain,
      "Plain GeoParquet",
      &format!(
        "wrote {} selected rows without optimized clustering",
        output_result.rows_written
      ),
    );
  }

  report_job_completion(&options, job_start);
  Ok(())
}

/// Open and validate source, destination, and DataFusion resources for one job.
async fn open_optimize_job(options: &OptimizeJobOptions) -> Result<OpenedOptimizeJob> {
  let input_format = resolve_source_format(&options.input, options.input_format)?;
  let input = open_input(
    input_format,
    &InputOpenOptions {
      location: options.input.clone(),
      layer: options.layer.clone(),
    },
  )
  .await?;
  let output_layout =
    resolve_output_layout(&options.output, options.output_files, options.overwrite)?;
  let source_schema = input.schema()?;
  validate_covering_configuration(options.covering, source_schema.as_ref())?;
  validate_internal_projection_columns(source_schema.as_ref())?;
  let discovered_rows = input.total_rows()?;
  let total_input_rows = options.row_range.effective_rows(discovered_rows);
  explain_run_configuration(
    options.explain,
    input.format_name(),
    options,
    discovered_rows,
    total_input_rows,
    output_layout.parts,
  );

  let session = new_datafusion_session()?;
  configure_explain_session(session.context(), options.explain);
  let input_dataframe =
    prepare_input_dataframe(input.as_ref(), session.context(), options, total_input_rows).await?;
  Ok(OpenedOptimizeJob {
    _session: session,
    input,
    output_layout,
    source_schema,
    total_input_rows,
    input_dataframe,
  })
}

async fn prepare_input_dataframe(
  input: &dyn InputSource,
  session: &engine::SessionContext,
  options: &OptimizeJobOptions,
  total_input_rows: u64,
) -> Result<engine::DataFrame> {
  let dataframe = input.to_dataframe(session, options.row_range).await?;
  if !should_cache_input_dataframe(options.row_range) {
    return Ok(dataframe);
  }

  let cache_bar = row_bar(options.progress, "Caching selected input", total_input_rows);
  let cache_start = Instant::now();
  let dataframe = dataframe.cache().await?;
  cache_bar.inc(total_input_rows);
  finish_row_bar(
    &cache_bar,
    total_input_rows,
    "Cached selected input".to_string(),
  );
  explain_timing(
    options.explain,
    "Caching selected input",
    cache_start.elapsed(),
  );
  Ok(dataframe)
}

fn should_cache_input_dataframe(row_range: RowRange) -> bool {
  row_range.num.is_some()
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
    "[explain] output_path={} output_wkid={} overwrite={} progress={}",
    options.output.display(),
    options.output_wkid,
    options.overwrite,
    options.progress,
  );
}

#[cfg(test)]
mod tests {
  use super::should_cache_input_dataframe;
  use crate::input::RowRange;

  #[test]
  fn caches_only_explicitly_bounded_selections() {
    assert!(!should_cache_input_dataframe(RowRange::default()));
    assert!(!should_cache_input_dataframe(RowRange {
      start: 10,
      num: None,
    }));
    assert!(should_cache_input_dataframe(RowRange {
      start: 10,
      num: Some(25),
    }));
  }
}

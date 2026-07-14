//! Opens validated resources and executes one concrete spatial pipeline.

use std::time::Instant;

use anyhow::{Result, bail};
use datafusion::dataframe::DataFrame;
use datafusion::execution::context::SessionContext;
use engine::{DataFusionSession, OutputLayout};

use crate::diagnostics::{configure_explain_session, explain_timing};
use crate::geoparquet::validate_covering_configuration;
use crate::input::{InputOpenOptions, InputSource, RowRange, open_input, resolve_source_format};
use crate::optimized::validate_internal_projection_columns;
use crate::output::{OutputMode, validate_output_wkid};
use crate::progress::{finish_row_bar, row_bar};

use super::{
  OptimizedPartitionedPipeline, OptimizedSingleFilePipeline, Pipeline, PipelineKind, PlainPipeline,
  SpatialPipelineOptions, SpatialPipelineResult, SpatialPipelineState,
};

/// Execute one spatial request through durable GeoParquet output.
pub async fn run(options: SpatialPipelineOptions) -> Result<SpatialPipelineResult> {
  Pipeline::new(options).await?.execute().await
}

impl Pipeline {
  async fn new(options: SpatialPipelineOptions) -> Result<Self> {
    validate_output_wkid(options.output.output_wkid);
    let started_at = Instant::now();
    let input_format = resolve_source_format(&options.input.location, options.input.format)?;
    let input = open_input(
      input_format,
      &InputOpenOptions::new(options.input.location.clone(), options.input.layer.clone()),
    )
    .await?;
    let output_layout = OutputLayout::new(
      &options.output.path,
      options.output.file_count,
      options.output.overwrite,
    )?;
    let source_schema = input.schema()?;
    validate_covering_configuration(options.output.covering, source_schema.as_ref())?;
    validate_internal_projection_columns(source_schema.as_ref())?;
    let discovered_rows = input.total_rows()?;
    let total_input_rows = options.input.row_range.effective_rows(discovered_rows);
    explain_run_configuration(
      options.execution.explain,
      input.format_name(),
      &options,
      discovered_rows,
      total_input_rows,
      output_layout.part_count(),
    );

    let session = DataFusionSession::new()?;
    configure_explain_session(session.context(), options.execution.explain);
    let input_dataframe = prepare_input_dataframe(
      input.as_ref(),
      session.context(),
      options.input.row_range,
      options.execution,
      total_input_rows,
    )
    .await?;
    let output_mode = options.output.mode;
    let state = SpatialPipelineState {
      started_at,
      _session: session,
      input,
      input_dataframe,
      output_layout,
      source_schema,
      total_input_rows,
      row_range: options.input.row_range,
      geometry_column: options.input.geometry_column,
      input_wkid: options.input.input_wkid,
      output_wkid: options.output.output_wkid,
      covering: options.output.covering,
      compression: options.output.compression,
      progress: options.execution.progress,
      explain: options.execution.explain,
    };

    match PipelineKind::new(output_mode, state.output_layout.part_count())? {
      PipelineKind::Plain => Ok(Self::Plain(PlainPipeline::new(state))),
      PipelineKind::OptimizedSingleFile => Ok(Self::OptimizedSingleFile(
        OptimizedSingleFilePipeline::new(state),
      )),
      PipelineKind::OptimizedPartitioned => Ok(Self::OptimizedPartitioned(
        OptimizedPartitionedPipeline::new(state),
      )),
    }
  }
}

impl PipelineKind {
  fn new(output_mode: OutputMode, output_parts: usize) -> Result<Self> {
    match (output_mode, output_parts) {
      (OutputMode::Plain, 1) => Ok(Self::Plain),
      (OutputMode::Plain, _) => {
        bail!("plain GeoParquet output does not support --output-files")
      }
      (OutputMode::Optimized, 1) => Ok(Self::OptimizedSingleFile),
      (OutputMode::Optimized, _) => Ok(Self::OptimizedPartitioned),
    }
  }
}

async fn prepare_input_dataframe(
  input: &dyn InputSource,
  session: &SessionContext,
  row_range: RowRange,
  execution: super::ExecutionOptions,
  total_input_rows: u64,
) -> Result<DataFrame> {
  let dataframe = input.to_dataframe(session, row_range).await?;
  if row_range.num().is_none() {
    return Ok(dataframe);
  }

  let cache_bar = row_bar(
    execution.progress,
    "Caching selected input",
    total_input_rows,
  );
  let cache_start = Instant::now();
  let dataframe = dataframe.cache().await?;
  cache_bar.inc(total_input_rows);
  finish_row_bar(
    &cache_bar,
    total_input_rows,
    "Cached selected input".to_string(),
  );
  explain_timing(
    execution.explain,
    "Caching selected input",
    cache_start.elapsed(),
  );
  Ok(dataframe)
}

fn explain_run_configuration(
  explain: bool,
  input_format: &str,
  options: &SpatialPipelineOptions,
  discovered_rows: u64,
  effective_rows: u64,
  output_parts: usize,
) {
  if !explain {
    return;
  }
  eprintln!(
    "[explain] input_format={input_format} discovered_rows={discovered_rows} effective_rows={effective_rows} output_files={output_parts} start={} num={}",
    options.input.row_range.start(),
    options
      .input
      .row_range
      .num()
      .map(|num| num.to_string())
      .unwrap_or_else(|| "all".to_string())
  );
  eprintln!(
    "[explain] output_path={} output_wkid={} overwrite={} progress={}",
    options.output.path.display(),
    options.output.output_wkid,
    options.output.overwrite,
    options.execution.progress,
  );
}

#[cfg(test)]
mod tests {
  use super::PipelineKind;
  use crate::output::OutputMode;

  #[test]
  fn selects_one_concrete_pipeline_kind() {
    assert_eq!(
      PipelineKind::new(OutputMode::Plain, 1).unwrap(),
      PipelineKind::Plain
    );
    assert_eq!(
      PipelineKind::new(OutputMode::Optimized, 1).unwrap(),
      PipelineKind::OptimizedSingleFile
    );
    assert_eq!(
      PipelineKind::new(OutputMode::Optimized, 2).unwrap(),
      PipelineKind::OptimizedPartitioned
    );
  }
}

//! Opens validated resources and selects one concrete spatial pipeline.

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Result, bail};
use engine::output_layout::resolve_output_layout;
use engine::session::new_datafusion_session;

use crate::diagnostics::{configure_explain_session, explain_timing};
use crate::geoparquet::validate_covering_configuration;
use crate::input::{
  InputOpenOptions, InputSource, RowRange, SourceFormat, open_input, resolve_source_format,
};
use crate::optimized::multiscale::validate_internal_projection_columns;
use crate::output::{GeoParquetOutputMode, validate_output_wkid};
use crate::progress::{finish_row_bar, row_bar};

use super::{
  OptimizedPartitionedPipeline, OptimizedSingleFilePipeline, PlainPipeline, SpatialPipeline,
  SpatialPipelineState,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PipelineKind {
  Plain,
  OptimizedSingleFile,
  OptimizedPartitioned,
}

/// Configures one complete spatial pipeline.
pub struct SpatialPipelineOptions {
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

pub(super) async fn build_pipeline(options: SpatialPipelineOptions) -> Result<SpatialPipeline> {
  validate_output_wkid(options.output_wkid);
  let started_at = Instant::now();
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
    &options,
    discovered_rows,
    total_input_rows,
    output_layout.parts,
  );

  let session = new_datafusion_session()?;
  configure_explain_session(session.context(), options.explain);
  let input_dataframe = prepare_input_dataframe(
    input.as_ref(),
    session.context(),
    &options,
    total_input_rows,
  )
  .await?;
  let output_mode = options.output_mode;
  let state = SpatialPipelineState {
    started_at,
    _session: session,
    input,
    input_dataframe,
    output_layout,
    source_schema,
    total_input_rows,
    row_range: options.row_range,
    geometry_column: options.geometry_column,
    input_wkid: options.input_wkid,
    output_wkid: options.output_wkid,
    covering: options.covering,
    compression: options.compression,
    progress: options.progress,
    explain: options.explain,
  };

  match pipeline_kind(output_mode, state.output_layout.parts)? {
    PipelineKind::Plain => Ok(SpatialPipeline::Plain(PlainPipeline::new(state))),
    PipelineKind::OptimizedSingleFile => Ok(SpatialPipeline::OptimizedSingleFile(
      OptimizedSingleFilePipeline::new(state),
    )),
    PipelineKind::OptimizedPartitioned => Ok(SpatialPipeline::OptimizedPartitioned(
      OptimizedPartitionedPipeline::new(state),
    )),
  }
}

fn pipeline_kind(output_mode: GeoParquetOutputMode, output_parts: usize) -> Result<PipelineKind> {
  match (output_mode, output_parts) {
    (GeoParquetOutputMode::Plain, 1) => Ok(PipelineKind::Plain),
    (GeoParquetOutputMode::Plain, _) => {
      bail!("plain GeoParquet output does not support --output-files")
    }
    (GeoParquetOutputMode::Optimized, 1) => Ok(PipelineKind::OptimizedSingleFile),
    (GeoParquetOutputMode::Optimized, _) => Ok(PipelineKind::OptimizedPartitioned),
  }
}

async fn prepare_input_dataframe(
  input: &dyn InputSource,
  session: &engine::SessionContext,
  options: &SpatialPipelineOptions,
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
  use super::{PipelineKind, pipeline_kind, should_cache_input_dataframe};
  use crate::input::RowRange;
  use crate::output::GeoParquetOutputMode;

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

  #[test]
  fn selects_one_concrete_pipeline_kind() {
    assert_eq!(
      pipeline_kind(GeoParquetOutputMode::Plain, 1).unwrap(),
      PipelineKind::Plain
    );
    assert_eq!(
      pipeline_kind(GeoParquetOutputMode::Optimized, 1).unwrap(),
      PipelineKind::OptimizedSingleFile
    );
    assert_eq!(
      pipeline_kind(GeoParquetOutputMode::Optimized, 4).unwrap(),
      PipelineKind::OptimizedPartitioned
    );
    assert!(pipeline_kind(GeoParquetOutputMode::Plain, 4).is_err());
  }
}

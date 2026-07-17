//! Opens validated resources and executes one concrete spatial pipeline.

use anyhow::{Result, bail};
use datafusion::dataframe::DataFrame;
use datafusion::execution::context::SessionContext;

use crate::input::{InputOpenOptions, InputSource, RowRange, open_input, resolve_source_format};
use crate::optimized::validate_internal_projection_columns;
use crate::output::{OutputMode, OutputPath, validate_output_wkid};
use crate::session::DataFusionSession;

use super::{
  OptimizedPartitionedPipeline, OptimizedSingleFilePipeline, OutputExecutionOptions, Pipeline,
  PipelineKind, PlainPipeline, SpatialPipelineOptions, SpatialPipelineResult, SpatialPipelineState,
};

/// Execute one spatial request through durable GeoParquet output.
pub async fn run(options: SpatialPipelineOptions) -> Result<SpatialPipelineResult> {
  Pipeline::new(options).await?.execute().await
}

impl Pipeline {
  async fn new(options: SpatialPipelineOptions) -> Result<Self> {
    validate_output_wkid(options.output.output_wkid);
    let input_format = resolve_source_format(&options.input.location, options.input.format)?;
    let input = open_input(
      input_format,
      &InputOpenOptions::new(options.input.location.clone(), options.input.layer.clone()),
    )
    .await?;
    let output_path = OutputPath::new(
      &options.output.path,
      options.output.file_count,
      options.output.overwrite,
    )?;
    let source_schema = input.schema()?;
    validate_internal_projection_columns(source_schema.as_ref())?;
    let discovered_rows = input.total_rows()?;
    let total_input_rows = options.input.row_range.effective_rows(discovered_rows);

    let session = DataFusionSession::new(options.memory_limit_bytes, options.target_partitions)?;
    let input_dataframe =
      prepare_input_dataframe(input.as_ref(), session.context(), options.input.row_range).await?;
    let output_mode = options.output.mode;
    let state = SpatialPipelineState {
      _session: session,
      input,
      input_dataframe,
      output_path,
      source_schema,
      total_input_rows,
      row_range: options.input.row_range,
      output_options: OutputExecutionOptions {
        geometry_column: options.input.geometry_column,
        input_wkid: options.input.input_wkid,
        output_wkid: options.output.output_wkid,
        covering: options.output.covering,
        strip_z: options.output.strip_z,
        strip_m: options.output.strip_m,
        multiscale_encoding: options.output.multiscale_encoding,
        compression: options.output.compression,
      },
      write_reporter: options.write_reporter,
      warning_store: Default::default(),
    };

    match PipelineKind::new(output_mode, state.output_path.part_count())? {
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
      (OutputMode::GeoParquet, 1) => Ok(Self::Plain),
      (OutputMode::GeoParquet, _) => {
        bail!("plain GeoParquet output does not support --output-files")
      }
      (OutputMode::OptimizedGeoParquet, 1) => Ok(Self::OptimizedSingleFile),
      (OutputMode::OptimizedGeoParquet, _) => Ok(Self::OptimizedPartitioned),
    }
  }
}

async fn prepare_input_dataframe(
  input: &dyn InputSource,
  session: &SessionContext,
  row_range: RowRange,
) -> Result<DataFrame> {
  let dataframe = input.to_dataframe(session, row_range).await?;
  if row_range.num().is_none() {
    return Ok(dataframe);
  }

  dataframe.cache().await.map_err(Into::into)
}

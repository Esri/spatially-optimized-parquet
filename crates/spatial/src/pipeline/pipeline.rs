use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Result, bail};
use arrow_schema::SchemaRef;
use datafusion::dataframe::DataFrame;
use datafusion::execution::context::SessionContext;

use crate::geoparquet::{GeoParquetWriter, SpatialReference};
use crate::input::{InputOpenOptions, InputSource, RowRange, SourceFormat, open_input};
use crate::optimized::{
  MultiscaleEncoding, partitioned, resolve_optimized_geoparquet, single,
  validate_internal_projection_columns,
};
use crate::output::{OutputMode, OutputPath};
use crate::session::DataFusionSession;

use super::{PipelineWarnings, SharedWriteReporter, SpatialPipelineResult, WriteReporter};

/// Configures one complete spatial pipeline.
#[derive(Clone)]
pub struct SpatialPipelineOptions {
  pub(super) input: InputOptions,
  pub(super) output: OutputOptions,
  pub(super) memory_limit_bytes: Option<usize>,
  pub(super) target_partitions: Option<usize>,
  pub(super) write_reporter: Option<SharedWriteReporter>,
}

impl SpatialPipelineOptions {
  /// Construct one spatial request from typed input and output sections.
  pub fn new(input: InputOptions, output: OutputOptions) -> Self {
    Self {
      input,
      output,
      memory_limit_bytes: None,
      target_partitions: None,
      write_reporter: None,
    }
  }

  /// Override the DataFusion memory-pool limit in bytes.
  pub fn with_memory_limit_bytes(mut self, memory_limit_bytes: usize) -> Self {
    self.memory_limit_bytes = Some(memory_limit_bytes);
    self
  }

  /// Override the DataFusion execution partition count.
  pub fn with_target_partitions(mut self, target_partitions: usize) -> Self {
    self.target_partitions = Some(target_partitions);
    self
  }

  /// Attach an optional callback for cumulative output row counts.
  pub fn with_write_reporter(mut self, reporter: impl WriteReporter + 'static) -> Self {
    self.write_reporter = Some(Arc::new(reporter));
    self
  }
}

/// Configures one input source and selected row range.
#[derive(Debug, Clone)]
pub struct InputOptions {
  pub(super) location: String,
  pub(super) format: Option<SourceFormat>,
  pub(super) row_range: RowRange,
  pub(super) layer: Option<String>,
  pub(super) geometry_column: Option<String>,
  pub(super) input_wkid: Option<u32>,
}

impl InputOptions {
  /// Construct input options from source, selection, and geometry overrides.
  pub fn new(
    location: impl Into<String>,
    format: Option<SourceFormat>,
    row_range: RowRange,
    layer: Option<String>,
    geometry_column: Option<String>,
    input_wkid: Option<u32>,
  ) -> Self {
    Self {
      location: location.into(),
      format,
      row_range,
      layer,
      geometry_column,
      input_wkid,
    }
  }

  pub(crate) fn geometry_column(&self) -> Option<&str> {
    self.geometry_column.as_deref()
  }

  pub(crate) fn input_wkid(&self) -> Option<u32> {
    self.input_wkid
  }
}

/// Configures one durable GeoParquet output.
#[derive(Debug, Clone)]
pub struct OutputOptions {
  pub(super) path: PathBuf,
  pub(super) mode: OutputMode,
  pub(super) file_count: Option<usize>,
  pub(super) compression: Option<String>,
  pub(super) output_wkid: u32,
  pub(super) covering: bool,
  pub(super) overwrite: bool,
  pub(super) strip_z: bool,
  pub(super) strip_m: bool,
  pub(super) multiscale_encoding: MultiscaleEncoding,
}

impl OutputOptions {
  /// Construct output options from path, product, storage, and CRS policy.
  pub fn new(
    path: impl Into<PathBuf>,
    mode: OutputMode,
    file_count: Option<usize>,
    compression: Option<String>,
    output_wkid: u32,
    covering: bool,
    overwrite: bool,
  ) -> Self {
    Self {
      path: path.into(),
      mode,
      file_count,
      compression,
      output_wkid,
      covering,
      overwrite,
      strip_z: false,
      strip_m: false,
      multiscale_encoding: MultiscaleEncoding::default(),
    }
  }

  /// Remove selected Z/M values from output geometry.
  pub fn with_stripped_dimensions(mut self, strip_z: bool, strip_m: bool) -> Self {
    self.strip_z = strip_z;
    self.strip_m = strip_m;
    self
  }

  /// Select the physical representation for optimized multiscale geometry.
  pub fn with_multiscale_encoding(mut self, encoding: MultiscaleEncoding) -> Self {
    self.multiscale_encoding = encoding;
    self
  }

  pub(crate) fn output_wkid(&self) -> u32 {
    self.output_wkid
  }

  pub(crate) fn strips_z(&self) -> bool {
    self.strip_z
  }

  pub(crate) fn strips_m(&self) -> bool {
    self.strip_m
  }

  pub(crate) fn multiscale_encoding(&self) -> MultiscaleEncoding {
    self.multiscale_encoding
  }
}

/// Executes one configured spatial pipeline.
pub struct Pipeline(PipelineExecution);

enum PipelineExecution {
  /// Runs plain GeoParquet output.
  GeoParquet(SpatialPipelineState),
  /// Runs optimized output for one file.
  OptimizedSingle(SpatialPipelineState),
  /// Runs optimized output across partition files.
  OptimizedPartitioned(SpatialPipelineState),
}

pub(crate) struct SpatialPipelineState {
  pub(super) _session: DataFusionSession,
  pub(super) input_source: Arc<dyn InputSource>,
  pub(super) input_dataframe: DataFrame,
  pub(super) output_path: OutputPath,
  pub(super) source_schema: SchemaRef,
  pub(super) total_input_rows: u64,
  pub(super) input_options: InputOptions,
  pub(super) output_options: OutputOptions,
  pub(super) write_reporter: Option<SharedWriteReporter>,
  pub(super) warnings: PipelineWarnings,
}

impl Pipeline {
  /// Execute one spatial request through durable GeoParquet output.
  pub async fn run(options: SpatialPipelineOptions) -> Result<SpatialPipelineResult> {
    Self::new(options).await?.execute().await
  }

  async fn new(options: SpatialPipelineOptions) -> Result<Self> {
    let SpatialPipelineOptions {
      input: input_options,
      output: output_options,
      memory_limit_bytes,
      target_partitions,
      write_reporter,
    } = options;
    SpatialReference::validate_output_wkid(output_options.output_wkid);
    let input_format = SourceFormat::resolve(&input_options.location, input_options.format)?;
    let input_source = open_input(
      input_format,
      &InputOpenOptions::new(input_options.location.clone(), input_options.layer.clone()),
    )
    .await?;
    let output_path = OutputPath::new(
      &output_options.path,
      output_options.file_count,
      output_options.overwrite,
    )?;
    let source_schema = input_source.schema()?;
    validate_internal_projection_columns(source_schema.as_ref())?;
    let discovered_rows = input_source.total_rows()?;
    let total_input_rows = input_options.row_range.effective_rows(discovered_rows);

    let session = DataFusionSession::new(memory_limit_bytes, target_partitions)?;
    let input_dataframe = Self::prepare_input_dataframe(
      input_source.as_ref(),
      session.context(),
      input_options.row_range,
    )
    .await?;
    let output_mode = output_options.mode;
    let state = SpatialPipelineState {
      _session: session,
      input_source,
      input_dataframe,
      output_path,
      source_schema,
      total_input_rows,
      input_options,
      output_options,
      write_reporter,
      warnings: Default::default(),
    };

    match (output_mode, state.output_path.part_count()) {
      (OutputMode::GeoParquet, 1) => Ok(Self(PipelineExecution::GeoParquet(state))),
      (OutputMode::GeoParquet, _) => {
        bail!("plain GeoParquet output does not support --output-files")
      }
      (OutputMode::OptimizedGeoParquet, 1) => Ok(Self(PipelineExecution::OptimizedSingle(state))),
      (OutputMode::OptimizedGeoParquet, _) => {
        Ok(Self(PipelineExecution::OptimizedPartitioned(state)))
      }
    }
  }

  pub(super) async fn execute(self) -> Result<SpatialPipelineResult> {
    match self.0 {
      PipelineExecution::GeoParquet(state) => Self::write_geoparquet(state).await,
      PipelineExecution::OptimizedSingle(state) => Self::write_optimized_single(state).await,
      PipelineExecution::OptimizedPartitioned(state) => {
        Self::write_optimized_partitioned(state).await
      }
    }
  }

  async fn write_geoparquet(state: SpatialPipelineState) -> Result<SpatialPipelineResult> {
    let options = &state.output_options;
    let rows_written = GeoParquetWriter::new(
      state.input_source.as_ref(),
      state.input_dataframe.clone(),
      &state.output_path,
      state.source_schema.as_ref(),
      state.input_options.geometry_column(),
      state.input_options.input_wkid(),
      state.input_options.row_range,
      state.total_input_rows,
      state.write_reporter.clone(),
    )
    .write(
      options.output_wkid,
      options.covering,
      options.strip_z,
      options.strip_m,
      options.compression.as_deref(),
    )
    .await?;
    Ok(state.finish(rows_written))
  }

  async fn write_optimized_single(state: SpatialPipelineState) -> Result<SpatialPipelineResult> {
    let (dataframe, optimization) = resolve_optimized_geoparquet(
      state.input_source.as_ref(),
      state.input_dataframe.clone(),
      state.source_schema.as_ref(),
      state.input_options.row_range,
      &state.input_options,
      &state.output_options,
    )
    .await?;
    let rows_written = single::write(
      dataframe,
      &state.output_path,
      state.source_schema.as_ref(),
      &optimization,
      state.output_options.covering,
      state.output_options.compression.as_deref(),
      state.total_input_rows,
      state.write_reporter.clone(),
      state.warnings.clone(),
    )
    .await?;
    Ok(state.finish(rows_written))
  }

  async fn write_optimized_partitioned(
    state: SpatialPipelineState,
  ) -> Result<SpatialPipelineResult> {
    let (dataframe, optimization) = resolve_optimized_geoparquet(
      state.input_source.as_ref(),
      state.input_dataframe.clone(),
      state.source_schema.as_ref(),
      state.input_options.row_range,
      &state.input_options,
      &state.output_options,
    )
    .await?;
    let rows_written = partitioned::write(
      dataframe,
      &state.output_path,
      state.source_schema.as_ref(),
      &optimization,
      state.output_options.covering,
      state.output_options.compression.as_deref(),
      state.total_input_rows,
      state.write_reporter.clone(),
      state.warnings.clone(),
    )
    .await?;
    Ok(state.finish(rows_written))
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
}

impl SpatialPipelineState {
  pub(super) fn finish(&self, rows_written: u64) -> SpatialPipelineResult {
    SpatialPipelineResult::new(self.total_input_rows, rows_written, &self.warnings)
  }
}

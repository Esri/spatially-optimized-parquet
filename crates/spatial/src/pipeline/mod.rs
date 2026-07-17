//! Connects one spatial request to its complete DataFusion execution path.

mod options;
mod reporter;
mod result;
mod runner;
mod write_geoparquet;
mod write_optimized_geoparquet;
mod write_optimized_geoparquet_partitioned;

use std::sync::Arc;

use anyhow::Result;
use arrow_schema::SchemaRef;
use datafusion::dataframe::DataFrame;

use crate::input::{InputSource, RowRange};
use crate::optimized::MultiscaleEncoding;
use crate::output::OutputPath;
use crate::session::DataFusionSession;

pub use options::{InputOptions, OutputOptions, SpatialPipelineOptions};
pub(crate) use reporter::SharedWriteReporter;
pub use reporter::{WriteProgress, WriteReporter};
pub(crate) use result::PipelineWarningStore;
pub use result::SpatialPipelineResult;

/// Executes one configured spatial pipeline.
pub enum Pipeline {
  /// Runs plain GeoParquet output.
  Plain(PlainPipeline),
  /// Runs optimized output for one file.
  OptimizedSingleFile(OptimizedSingleFilePipeline),
  /// Runs optimized output across partition files.
  OptimizedPartitioned(OptimizedPartitionedPipeline),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PipelineKind {
  Plain,
  OptimizedSingleFile,
  OptimizedPartitioned,
}

/// Executes the plain GeoParquet output path for one prepared request.
pub struct PlainPipeline {
  state: SpatialPipelineState,
}

/// Executes the optimized single-file output path for one prepared request.
pub struct OptimizedSingleFilePipeline {
  state: SpatialPipelineState,
}

/// Executes the optimized partitioned output path for one prepared request.
pub struct OptimizedPartitionedPipeline {
  state: SpatialPipelineState,
}

struct SpatialPipelineState {
  _session: DataFusionSession,
  input: Arc<dyn InputSource>,
  input_dataframe: DataFrame,
  output_path: OutputPath,
  source_schema: SchemaRef,
  total_input_rows: u64,
  row_range: RowRange,
  output_options: OutputExecutionOptions,
  write_reporter: Option<SharedWriteReporter>,
  warning_store: PipelineWarningStore,
}

pub(crate) struct OutputExecutionOptions {
  pub(crate) geometry_column: Option<String>,
  pub(crate) input_wkid: Option<u32>,
  pub(crate) output_wkid: u32,
  pub(crate) covering: bool,
  pub(crate) strip_z: bool,
  pub(crate) strip_m: bool,
  pub(crate) multiscale_encoding: MultiscaleEncoding,
  pub(crate) compression: Option<String>,
}

impl Pipeline {
  async fn execute(self) -> Result<SpatialPipelineResult> {
    match self {
      Self::Plain(pipeline) => pipeline.execute().await,
      Self::OptimizedSingleFile(pipeline) => pipeline.execute().await,
      Self::OptimizedPartitioned(pipeline) => pipeline.execute().await,
    }
  }
}

impl PlainPipeline {
  fn new(state: SpatialPipelineState) -> Self {
    Self { state }
  }
}

impl OptimizedSingleFilePipeline {
  fn new(state: SpatialPipelineState) -> Self {
    Self { state }
  }
}

impl OptimizedPartitionedPipeline {
  fn new(state: SpatialPipelineState) -> Self {
    Self { state }
  }
}

impl SpatialPipelineState {
  fn finish(&self, rows_written: u64) -> SpatialPipelineResult {
    SpatialPipelineResult::new(self.total_input_rows, rows_written, &self.warning_store)
  }
}

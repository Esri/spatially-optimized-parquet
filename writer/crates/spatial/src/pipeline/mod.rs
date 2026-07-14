//! Connects one spatial request to its complete DataFusion execution path.

mod optimized;
mod optimized_partitioned;
mod optimized_single_file;
mod options;
mod plain;
mod result;
mod runner;

use std::io::IsTerminal;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use arrow_schema::SchemaRef;
use datafusion::dataframe::DataFrame;
use engine::{DataFusionSession, OutputLayout};

use crate::diagnostics::explain_timing;
use crate::input::{InputSource, RowRange};
use crate::progress::format_elapsed;

pub use options::{ExecutionOptions, InputOptions, OutputOptions, SpatialPipelineOptions};
pub use result::SpatialPipelineResult;
pub use runner::run;

enum Pipeline {
  Plain(PlainPipeline),
  OptimizedSingleFile(OptimizedSingleFilePipeline),
  OptimizedPartitioned(OptimizedPartitionedPipeline),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PipelineKind {
  Plain,
  OptimizedSingleFile,
  OptimizedPartitioned,
}

struct PlainPipeline {
  state: SpatialPipelineState,
}

struct OptimizedSingleFilePipeline {
  state: SpatialPipelineState,
}

struct OptimizedPartitionedPipeline {
  state: SpatialPipelineState,
}

struct SpatialPipelineState {
  started_at: Instant,
  _session: DataFusionSession,
  input: Arc<dyn InputSource>,
  input_dataframe: DataFrame,
  output_layout: OutputLayout,
  source_schema: SchemaRef,
  total_input_rows: u64,
  row_range: RowRange,
  geometry_column: Option<String>,
  input_wkid: Option<u32>,
  output_wkid: u32,
  covering: bool,
  compression: Option<String>,
  progress: bool,
  explain: bool,
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
  fn finish_plain(&self, rows_written: u64) -> SpatialPipelineResult {
    self.finish_logging();
    SpatialPipelineResult::new(rows_written, None)
  }

  fn finish_validated(&self, rows_written: u64) -> Result<SpatialPipelineResult> {
    let report = crate::validate::validate(self.output_layout.path())?
      .ensure_valid()
      .map_err(anyhow::Error::new)?;
    self.finish_logging();
    Ok(SpatialPipelineResult::new(rows_written, Some(report)))
  }

  fn finish_logging(&self) {
    if self.progress && std::io::stderr().is_terminal() {
      eprintln!("Completed in {}", format_elapsed(self.started_at.elapsed()));
    }
    explain_timing(self.explain, "Total job", self.started_at.elapsed());
  }
}

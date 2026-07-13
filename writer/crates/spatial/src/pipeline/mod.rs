//! Connects one spatial request to its complete DataFusion execution path.

mod build;
mod optimized;
mod optimized_partitioned;
mod optimized_single_file;
mod plain;

use std::io::IsTerminal;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use arrow_schema::SchemaRef;
use engine::output_layout::OutputLayout;
use engine::session::DataFusionSession;

use crate::diagnostics::explain_timing;
use crate::input::{InputSource, RowRange};
use crate::progress::format_elapsed;

pub use build::SpatialPipelineOptions;

/// Represents one complete spatial execution path from opened input through durable output.
pub enum SpatialPipeline {
  /// Writes normalized GeoParquet without optimized clustering.
  Plain(PlainPipeline),
  /// Writes globally sorted optimized GeoParquet to one file.
  OptimizedSingleFile(OptimizedSingleFilePipeline),
  /// Writes range-partitioned optimized GeoParquet through concurrent sinks.
  OptimizedPartitioned(OptimizedPartitionedPipeline),
}

/// Executes normalized GeoParquet through the standard single-file writer.
pub struct PlainPipeline {
  state: SpatialPipelineState,
}

/// Executes globally sorted optimized GeoParquet through the standard single-file writer.
pub struct OptimizedSingleFilePipeline {
  state: SpatialPipelineState,
}

/// Executes range-partitioned optimized GeoParquet through the custom physical writer.
pub struct OptimizedPartitionedPipeline {
  state: SpatialPipelineState,
}

/// Represents the durable result produced by one spatial pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpatialPipelineResult {
  /// Stores the number of rows accepted by the output writer.
  pub rows_written: u64,
}

pub(crate) struct SpatialPipelineState {
  pub(crate) started_at: Instant,
  pub(crate) _session: DataFusionSession,
  pub(crate) input: Arc<dyn InputSource>,
  pub(crate) input_dataframe: engine::DataFrame,
  pub(crate) output_layout: OutputLayout,
  pub(crate) source_schema: SchemaRef,
  pub(crate) total_input_rows: u64,
  pub(crate) row_range: RowRange,
  pub(crate) geometry_column: Option<String>,
  pub(crate) input_wkid: Option<u32>,
  pub(crate) output_wkid: u32,
  pub(crate) covering: bool,
  pub(crate) compression: Option<String>,
  pub(crate) progress: bool,
  pub(crate) explain: bool,
}

impl SpatialPipeline {
  /// Build and execute one complete spatial pipeline.
  pub async fn run(options: SpatialPipelineOptions) -> Result<SpatialPipelineResult> {
    Self::build(options).await?.execute().await
  }

  /// Build one concrete pipeline while preserving existing input preparation behavior.
  pub async fn build(options: SpatialPipelineOptions) -> Result<Self> {
    build::build_pipeline(options).await
  }

  /// Execute the selected spatial and DataFusion pipeline.
  pub async fn execute(self) -> Result<SpatialPipelineResult> {
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
    if self.progress && std::io::stderr().is_terminal() {
      eprintln!("Completed in {}", format_elapsed(self.started_at.elapsed()));
    }
    explain_timing(self.explain, "Total job", self.started_at.elapsed());
    SpatialPipelineResult { rows_written }
  }
}

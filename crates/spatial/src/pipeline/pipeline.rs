// Copyright 2026 Esri
//
// Licensed under the Apache License Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Converts one spatial dataset into GeoParquet.
//!
//! Build a request with [`SpatialPipelineOptions`], then pass it to [`Pipeline::run`].
//! [`InputOptions`] says where the source data comes from and how to interpret it.
//! [`OutputOptions`] says where to write the result and whether it should use the optimized
//! GeoParquet layout.
//!
//! The pipeline opens the source, selects the requested [`RowRange`], and finds the geometry
//! column and its coordinate reference system. It then reprojects geometries when needed, can
//! remove Z or M values, and creates bounding boxes when the output needs them. Finally, it writes
//! either one GeoParquet file or an optimized set of partitioned files.
//!
//! The returned [`SpatialPipelineResult`] confirms that writing finished. It includes the expected
//! and actual row counts plus warnings about recoverable decisions made while preparing the data.
//! Invalid input, missing spatial metadata, unsupported geometry, planning failures, and write
//! failures return errors instead.
//!
//! The pipeline writes directly to the requested destination. When `overwrite` is enabled, it
//! removes an existing destination before work begins. If a write fails, it tries to remove the
//! files it created. Optimized output checks its own geometry, coordinate reference, clustering,
//! and reserved-column rules while it is built. Run [`crate::validate()`] afterward when you need
//! a separate, durable conformance report for completed output.
//!
//! # Example
//!
//! ```no_run
//! use spatial::{
//!   InputOptions, OutputMode, OutputOptions, Pipeline, RowRange, SpatialPipelineOptions,
//! };
//!
//! # async fn convert() -> Result<(), spatial::PipelineError> {
//! let request = SpatialPipelineOptions {
//!   input: InputOptions {
//!     location: "roads.parquet".to_string(),
//!     row_range: RowRange::new(500, Some(1_000)),
//!     ..Default::default()
//!   },
//!   output: OutputOptions {
//!     path: "roads.optimized.parquet".into(),
//!     mode: OutputMode::Optimized,
//!     compression: Some("gzip".to_string()),
//!     covering: true,
//!     overwrite: true,
//!     ..Default::default()
//!   },
//!   ..Default::default()
//! };
//! let result = Pipeline::run(request).await?;
//!
//! assert_eq!(result.rows_written(), result.rows_expected());
//! # Ok(())
//! # }
//! ```

use std::path::PathBuf;
use std::sync::Arc;

use arrow_schema::SchemaRef;
use datafusion::dataframe::DataFrame;
use datafusion::execution::context::SessionContext;

use super::{PipelineError, SpatialWriteContext};
use crate::geoparquet::SpatialReference;
use crate::input::{InputOpenOptions, InputSource, RowRange, SourceFormat, open_input};
use crate::optimized::{MultiscaleEncoding, OptimizedLayout};
use crate::output::{OutputMode, OutputPath, PlainWriter, partitioned, single};
use crate::session::DataFusionSession;

use super::{PipelineWarnings, SharedWriteReporter, SpatialPipelineResult, WriteReporter};

/// Configures one [`Pipeline`] run.
#[derive(Clone)]
pub struct SpatialPipelineOptions {
  /// Source location, selection, and geometry settings.
  pub input: InputOptions,
  /// Destination and output settings.
  pub output: OutputOptions,
  /// Optional DataFusion memory limit in bytes.
  pub memory_limit_bytes: Option<usize>,
  /// Optional DataFusion execution partition count, unrelated to output file count.
  pub target_partitions: Option<usize>,
  /// Optional callback for cumulative written-row counts.
  pub write_reporter: Option<Arc<dyn WriteReporter>>,
}

impl Default for SpatialPipelineOptions {
  fn default() -> Self {
    Self {
      input: InputOptions::default(),
      output: OutputOptions::default(),
      memory_limit_bytes: None,
      target_partitions: None,
      write_reporter: None,
    }
  }
}

impl SpatialPipelineOptions {
  /// Create the default, unconfigured request.
  pub fn new() -> Self {
    Self::default()
  }
}

/// Configures how [`Pipeline`] reads source data.
#[derive(Debug, Clone)]
pub struct InputOptions {
  /// Local path or HTTP URL for the source dataset.
  pub location: String,
  /// Optional source format that overrides inference from the location.
  pub format: Option<SourceFormat>,
  /// Source rows to process. Defaults to every row.
  pub row_range: RowRange,
  /// Optional layer name for multi-layer sources such as GeoPackage.
  pub layer: Option<String>,
  /// Optional WKB geometry column that overrides source metadata.
  pub geometry_column: Option<String>,
  /// Optional EPSG WKID for geometry without coordinate-reference metadata.
  pub input_wkid: Option<u32>,
}

impl Default for InputOptions {
  fn default() -> Self {
    Self {
      location: String::new(),
      format: None,
      row_range: RowRange::default(),
      layer: None,
      geometry_column: None,
      input_wkid: None,
    }
  }
}

impl InputOptions {
  /// Create the default, unconfigured input settings.
  pub fn new() -> Self {
    Self::default()
  }
}

/// Configures the GeoParquet output written by [`Pipeline`].
#[derive(Debug, Clone)]
pub struct OutputOptions {
  /// Output file or dataset-directory destination.
  pub path: PathBuf,
  /// Plain or optimized GeoParquet. Defaults to optimized output.
  pub mode: OutputMode,
  /// Number of files for a directory destination.
  pub file_count: Option<usize>,
  /// Optional Parquet codec name. `None` selects Snappy.
  pub compression: Option<String>,
  /// EPSG WKID written to output geometry and metadata.
  pub output_wkid: u32,
  /// Optional extent used to normalize spatial cluster keys.
  pub normalization_extent: Option<[f64; 4]>,
  /// Z bit width for points or XZ maximum level for non-point geometry.
  pub cluster_depth: u32,
  /// Include the canonical GeoParquet bounding-box covering column.
  pub covering: bool,
  /// Allow replacement of an existing destination.
  pub overwrite: bool,
  /// Remove Z ordinates from output geometry and metadata.
  pub strip_z: bool,
  /// Remove M ordinates from output geometry and metadata.
  pub strip_m: bool,
  /// Encoding for optimized complex-geometry multiscale levels.
  pub multiscale_encoding: MultiscaleEncoding,
  /// Adds SOP `geodisplay` metadata to optimized output.
  pub write_sop: bool,
  /// Adds draft GeoParquet ordering and level-of-detail metadata to optimized output.
  pub write_extensions: bool,
}

impl Default for OutputOptions {
  fn default() -> Self {
    Self {
      path: PathBuf::new(),
      mode: OutputMode::default(),
      file_count: None,
      compression: None,
      output_wkid: crate::DEFAULT_OUTPUT_WKID,
      normalization_extent: None,
      cluster_depth: 20,
      covering: false,
      overwrite: false,
      strip_z: false,
      strip_m: false,
      multiscale_encoding: MultiscaleEncoding::default(),
      write_sop: true,
      write_extensions: false,
    }
  }
}

impl OutputOptions {
  /// Create the default, unconfigured output settings.
  pub fn new() -> Self {
    Self::default()
  }
}

/// Runs one configured spatial conversion.
pub struct Pipeline(PipelineExecution);

/// Selects the writer after shared setup completes.
enum PipelineExecution {
  /// Runs plain GeoParquet output.
  Plain(SpatialPipelineState),
  /// Runs optimized output for one file.
  OptimizedSingle(SpatialPipelineState),
  /// Runs optimized output across partition files.
  OptimizedPartitioned(SpatialPipelineState),
}

/// Retains state shared by the selected writer.
struct SpatialPipelineState {
  _session: DataFusionSession,
  input_source: Arc<dyn InputSource>,
  input_dataframe: DataFrame,
  output_path: OutputPath,
  source_schema: SchemaRef,
  total_input_rows: u64,
  input_options: InputOptions,
  output_options: OutputOptions,
  write_reporter: Option<SharedWriteReporter>,
  warnings: PipelineWarnings,
}

impl Pipeline {
  /// Convert the configured source into GeoParquet.
  ///
  /// # Errors
  ///
  /// Returns an error for invalid configuration, unsupported source data, planning failures, or
  /// failed writes. A failed write tries to remove files it created.
  ///
  pub async fn run(
    options: SpatialPipelineOptions,
  ) -> Result<SpatialPipelineResult, PipelineError> {
    Self::new(options).await?.execute().await
  }

  async fn new(options: SpatialPipelineOptions) -> Result<Self, PipelineError> {
    let SpatialPipelineOptions {
      input: input_options,
      output: output_options,
      memory_limit_bytes,
      target_partitions,
      write_reporter,
    } = options;
    SpatialReference::validate_output_wkid(output_options.output_wkid)?;
    if output_options.cluster_depth == 0 || output_options.cluster_depth > 32 {
      return Err(PipelineError::InvalidRequest(
        "cluster depth must be between 1 and 32".to_string(),
      ));
    }
    if let Some([xmin, ymin, xmax, ymax]) = output_options.normalization_extent
      && (![xmin, ymin, xmax, ymax]
        .iter()
        .all(|value| value.is_finite())
        || xmin >= xmax
        || ymin >= ymax)
    {
      return Err(PipelineError::InvalidRequest(
        "normalization extent must contain finite xmin ymin xmax ymax values with positive width and height"
          .to_string(),
      ));
    }
    if output_options.mode == OutputMode::Plain && output_options.write_extensions {
      return Err(PipelineError::InvalidRequest(
        "--write-extensions cannot be combined with --no-optimization".to_string(),
      ));
    }
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
      (OutputMode::Plain, 1) => Ok(Self(PipelineExecution::Plain(state))),
      (OutputMode::Plain, _) => Err(PipelineError::InvalidRequest(
        "plain GeoParquet output does not support --partitions".to_string(),
      )),
      (OutputMode::Optimized, 1) => Ok(Self(PipelineExecution::OptimizedSingle(state))),
      (OutputMode::Optimized, _) => Ok(Self(PipelineExecution::OptimizedPartitioned(state))),
    }
  }

  async fn execute(self) -> Result<SpatialPipelineResult, PipelineError> {
    match self.0 {
      PipelineExecution::Plain(state) => Self::write_plain(state).await,
      PipelineExecution::OptimizedSingle(state) => Self::write_optimized_single(state).await,
      PipelineExecution::OptimizedPartitioned(state) => {
        Self::write_optimized_partitioned(state).await
      }
    }
  }

  async fn write_plain(
    state: SpatialPipelineState,
  ) -> Result<SpatialPipelineResult, PipelineError> {
    let options = &state.output_options;
    let context = Self::resolve_spatial_write_context(&state).await?;
    let rows_written = PlainWriter::new(
      &context,
      &state.output_path,
      state.source_schema.as_ref(),
      state.total_input_rows,
      state.write_reporter.clone(),
    )
    .write(options.covering, options.compression.as_deref())
    .await?;
    Ok(state.finish(rows_written))
  }

  async fn write_optimized_single(
    state: SpatialPipelineState,
  ) -> Result<SpatialPipelineResult, PipelineError> {
    let context = Self::resolve_spatial_write_context(&state).await?;
    let layout = OptimizedLayout::new(&context, &state.output_options)?;
    let rows_written = single::write(
      &state.output_path,
      state.source_schema.as_ref(),
      &context,
      &layout,
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
  ) -> Result<SpatialPipelineResult, PipelineError> {
    let context = Self::resolve_spatial_write_context(&state).await?;
    let layout = OptimizedLayout::new(&context, &state.output_options)?;
    let rows_written = partitioned::write(
      &state.output_path,
      state.source_schema.as_ref(),
      &context,
      &layout,
      state.output_options.covering,
      state.output_options.compression.as_deref(),
      state.total_input_rows,
      state.write_reporter.clone(),
      state.warnings.clone(),
    )
    .await?;
    Ok(state.finish(rows_written))
  }

  async fn resolve_spatial_write_context(
    state: &SpatialPipelineState,
  ) -> Result<SpatialWriteContext, PipelineError> {
    SpatialWriteContext::resolve(
      state.input_source.as_ref(),
      state.input_dataframe.clone(),
      state.source_schema.as_ref(),
      state.input_options.geometry_column.as_deref(),
      state.input_options.input_wkid,
      state.input_options.row_range,
      state.output_options.output_wkid,
      state.output_options.strip_z,
      state.output_options.strip_m,
      state.output_options.normalization_extent,
    )
    .await
    .map_err(|error| PipelineError::InvalidRequest(error.to_string()))
  }

  async fn prepare_input_dataframe(
    input: &dyn InputSource,
    session: &SessionContext,
    row_range: RowRange,
  ) -> Result<DataFrame, PipelineError> {
    let dataframe = input.to_dataframe(session, row_range).await?;
    if row_range.num().is_none() {
      return Ok(dataframe);
    }

    dataframe
      .cache()
      .await
      .map_err(|source| PipelineError::DataFusion {
        operation: "cache limited input dataframe",
        source,
      })
  }
}

impl SpatialPipelineState {
  fn finish(&self, rows_written: u64) -> SpatialPipelineResult {
    SpatialPipelineResult::new(self.total_input_rows, rows_written, &self.warnings)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn spatial_pipeline_options_default_and_new_preserve_unconfigured_request_policy() {
    for options in [
      SpatialPipelineOptions::default(),
      SpatialPipelineOptions::new(),
    ] {
      assert!(options.input.location.is_empty());
      assert_eq!(options.output.path, PathBuf::new());
      assert_eq!(options.memory_limit_bytes, None);
      assert_eq!(options.target_partitions, None);
      assert!(options.write_reporter.is_none());
    }
  }

  #[test]
  fn input_options_default_and_new_preserve_unconfigured_input_policy() {
    for options in [InputOptions::default(), InputOptions::new()] {
      assert!(options.location.is_empty());
      assert_eq!(options.format, None);
      assert_eq!(options.row_range, RowRange::default());
      assert_eq!(options.layer, None);
      assert_eq!(options.geometry_column, None);
      assert_eq!(options.input_wkid, None);
    }
  }

  #[test]
  fn output_options_default_and_new_preserve_safe_output_policy() {
    for options in [OutputOptions::default(), OutputOptions::new()] {
      assert_eq!(options.path, PathBuf::new());
      assert_eq!(options.mode, OutputMode::Optimized);
      assert_eq!(options.file_count, None);
      assert_eq!(options.compression, None);
      assert_eq!(options.output_wkid, crate::DEFAULT_OUTPUT_WKID);
      assert!(!options.covering);
      assert!(!options.overwrite);
      assert!(!options.strip_z);
      assert!(!options.strip_m);
      assert_eq!(options.multiscale_encoding, MultiscaleEncoding::Pbf);
      assert!(options.write_sop);
      assert!(!options.write_extensions);
    }
  }
}

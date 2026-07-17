//! Coordinates one spatial conversion from caller-owned options to completed GeoParquet output.
//!
//! [`SpatialPipelineOptions`] owns the request-wide execution policy and the two domain-specific
//! sections: [`InputOptions`] describes how to open and interpret the source, while
//! [`OutputOptions`] describes the output product and filesystem policy. [`Pipeline::run`]
//! consumes that complete request. Its private [`PipelineExecution`] then selects one writer, and
//! [`SpatialPipelineState`] keeps the input provider, DataFusion session, normalized selection,
//! output destination, reporting callback, and warnings alive through that writer's lifetime.
//!
//! The lifecycle follows these phases:
//!
//! 1. Resolve or infer the input format, open the source, validate reserved column names, and
//!    resolve the output path. Existing output is removed at this point when overwrite is enabled.
//! 2. Create a [`DataFusionSession`] with the requested memory and partition policy, then ask the
//!    input provider for the selected [`RowRange`]. Bounded selections are cached because geometry
//!    discovery, extent analysis, optimization, and writing may consume the same rows.
//! 3. Resolve the geometry column and source coordinate reference system from metadata or explicit
//!    overrides. The selected geometry is reprojected to the output reference, requested Z/M
//!    dimensions are stripped, and a canonical bounding-box column is reused or computed.
//! 4. Route a one-part plain request to the GeoParquet writer. Route optimized output to the
//!    globally ordered single-file writer when the resolved part count is one, or to the
//!    range-partitioned writer when it exceeds one.
//! 5. Wait for the Parquet sink to complete, then return [`SpatialPipelineResult`] with the
//!    expected row count, authoritative written row count, and deduplicated warnings.
//!
//! Output writes target the resolved destination directly rather than using a transactional
//! replacement. A failed sink attempts to delete files it created, and reports cleanup failure
//! alongside the write failure. Optimized execution validates source geometry, coordinate
//! reference, clustering, and reserved-column invariants while planning. It does not run the
//! public [`crate::validate()`] dataset validator after writing. Call that function when a durable
//! conformance report is required.
//!
//! Progress and warnings use separate channels. A [`WriteReporter`] receives monotonic cumulative
//! counts while batches reach the Parquet sink and receives a final authoritative count. Warnings
//! describe recoverable normalization decisions, remain deduplicated across parallel execution,
//! and appear only in a successful [`SpatialPipelineResult`]. Invalid configuration, unresolved
//! geometry or spatial reference, unsupported geometry, DataFusion planning, and write failures
//! return errors instead.
//!
//! # Example
//!
//! ```no_run
//! use spatial::{
//!   DEFAULT_OUTPUT_WKID, InputOptions, OutputMode, OutputOptions, Pipeline, RowRange,
//!   SpatialPipelineOptions,
//! };
//!
//! # async fn convert() -> anyhow::Result<()> {
//! let input = InputOptions::new(
//!   "roads.parquet",
//!   None,
//!   RowRange::default(),
//!   None,
//!   None,
//!   None,
//! );
//! let output = OutputOptions::new(
//!   "roads.optimized.parquet",
//!   OutputMode::OptimizedGeoParquet,
//!   None,
//!   Some("zstd".to_string()),
//!   DEFAULT_OUTPUT_WKID,
//!   true,
//!   false,
//! );
//! let result = Pipeline::run(SpatialPipelineOptions::new(input, output)).await?;
//!
//! assert_eq!(result.rows_written(), result.rows_expected());
//! # Ok(())
//! # }
//! ```

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

/// Owns the input, output, resource, and reporting policy for one pipeline execution.
///
/// The value owns [`InputOptions`] and [`OutputOptions`] until [`Pipeline::run`] consumes it.
/// Cloning the request also clones the optional reporter's shared handle, so cloned requests may
/// report to the same callback.
#[derive(Clone)]
pub struct SpatialPipelineOptions {
  input: InputOptions,
  output: OutputOptions,
  memory_limit_bytes: Option<usize>,
  target_partitions: Option<usize>,
  write_reporter: Option<SharedWriteReporter>,
}

impl SpatialPipelineOptions {
  /// Build one spatial request from independently configured input and output sections.
  ///
  /// DataFusion uses its default target partition count and a memory limit equal to half of
  /// physical memory unless the corresponding builder methods override them. Reporting remains
  /// disabled until [`Self::with_write_reporter`] attaches a callback.
  pub fn new(input: InputOptions, output: OutputOptions) -> Self {
    Self {
      input,
      output,
      memory_limit_bytes: None,
      target_partitions: None,
      write_reporter: None,
    }
  }

  /// Configure the DataFusion memory-pool limit in bytes for analysis, sorting, and writing.
  ///
  /// The runtime spills eligible work to a session-owned temporary directory when the bounded
  /// pool cannot retain it. Passing zero causes [`Pipeline::run`] to return an error.
  pub fn with_memory_limit_bytes(mut self, memory_limit_bytes: usize) -> Self {
    self.memory_limit_bytes = Some(memory_limit_bytes);
    self
  }

  /// Configure the target DataFusion execution partition count.
  ///
  /// This controls execution parallelism, not the number of output files. Use
  /// [`OutputOptions::new`]'s `file_count` argument for output topology. Passing zero causes
  /// [`Pipeline::run`] to return an error.
  pub fn with_target_partitions(mut self, target_partitions: usize) -> Self {
    self.target_partitions = Some(target_partitions);
    self
  }

  /// Attach a thread-safe callback for cumulative rows accepted by the Parquet sink.
  ///
  /// Parallel writers may produce updates from worker threads. Delivered counts never decrease,
  /// and successful execution forces one final update with the authoritative written count.
  /// Reporting observes writes only and does not receive planning phases, warnings, or errors.
  pub fn with_write_reporter(mut self, reporter: impl WriteReporter + 'static) -> Self {
    self.write_reporter = Some(Arc::new(reporter));
    self
  }
}

/// Owns source location, row selection, layer selection, and geometry interpretation policy.
///
/// Format inference recognizes local directories as Parquet datasets and otherwise uses the final
/// path extension of a local path or URL. An explicit [`SourceFormat`] bypasses inference.
/// `layer` selects a GeoPackage layer. `geometry_column` overrides source metadata when the
/// intended WKB column cannot be inferred.
#[derive(Debug, Clone)]
pub struct InputOptions {
  location: String,
  format: Option<SourceFormat>,
  row_range: RowRange,
  layer: Option<String>,
  geometry_column: Option<String>,
  input_wkid: Option<u32>,
}

impl InputOptions {
  /// Build input policy from source, selection, and optional geometry overrides.
  ///
  /// `row_range` selects a zero-based contiguous range before geometry analysis and output.
  /// `input_wkid` supplies an EPSG identifier only when the selected geometry lacks coordinate
  /// reference metadata. Supplying it when metadata already defines a reference returns an error,
  /// preventing two competing source references.
  ///
  /// A missing or ambiguous geometry column, an unknown format without `format`, an unavailable
  /// layer, or an unresolvable coordinate reference causes [`Pipeline::run`] to return an error.
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

/// Owns output topology, encoding, coordinate reference, and replacement policy.
///
/// A path with an extension denotes one file. A path without an extension denotes a directory and
/// requires `file_count`. A resolved count of one selects single-output execution. Counts greater
/// than one select partitioned optimized execution and are rejected for plain GeoParquet.
///
/// `covering` includes the canonical GeoParquet bounding-box covering column in durable output.
/// `overwrite` authorizes removal of an existing compatible destination during pipeline setup,
/// before geometry planning and writing complete.
#[derive(Debug, Clone)]
pub struct OutputOptions {
  path: PathBuf,
  mode: OutputMode,
  file_count: Option<usize>,
  compression: Option<String>,
  output_wkid: u32,
  covering: bool,
  overwrite: bool,
  strip_z: bool,
  strip_m: bool,
  multiscale_encoding: MultiscaleEncoding,
}

impl OutputOptions {
  /// Build output policy from destination, product, storage, and coordinate-reference choices.
  ///
  /// `compression` accepts `snappy`, `gzip`, `brotli`, `lz4`, `lz4_raw`, `zstd`, or
  /// `uncompressed`. Omitting it selects Snappy. `output_wkid` identifies the EPSG reference used
  /// for geometry, extents, and GeoParquet metadata. The pipeline currently implements only
  /// [`crate::DEFAULT_OUTPUT_WKID`].
  ///
  /// `file_count` must be at least one. File destinations accept only one part, while directory
  /// destinations require an explicit count. Existing destinations return an error unless
  /// `overwrite` authorizes replacement.
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

  /// Configure removal of Z and M ordinates from geometry and output metadata.
  ///
  /// Stripping occurs after source geometry resolution and reprojection is planned, before
  /// bounding boxes, clustering columns, and multiscale payloads are derived. Each flag operates
  /// independently, and absent dimensions remain absent.
  pub fn with_stripped_dimensions(mut self, strip_z: bool, strip_m: bool) -> Self {
    self.strip_z = strip_z;
    self.strip_m = strip_m;
    self
  }

  /// Select the physical representation for optimized complex-geometry multiscale levels.
  ///
  /// This setting affects optimized multipoint, polyline, and polygon payloads. Point geometry
  /// uses scalar coordinate columns and Z-order clustering instead.
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

/// Owns one prepared execution route and its phase-spanning spatial state.
///
/// Callers normally use [`Pipeline::run`], which constructs this private route after validating
/// request-level invariants. The value cannot be reused because execution consumes its DataFusion
/// plan, output destination, and warning collector.
pub struct Pipeline(PipelineExecution);

/// Selects the writer that exclusively owns the execution phase after common setup.
enum PipelineExecution {
  /// Runs plain GeoParquet output.
  GeoParquet(SpatialPipelineState),
  /// Runs optimized output for one file.
  OptimizedSingle(SpatialPipelineState),
  /// Runs optimized output across partition files.
  OptimizedPartitioned(SpatialPipelineState),
}

/// Owns resources and resolved request state shared by exactly one selected writer.
///
/// Retaining `_session` keeps DataFusion spill storage alive until all lazy plans finish.
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
  /// Execute one spatial request through completed GeoParquet output.
  ///
  /// Setup normalizes the source into a DataFusion frame, resolves geometry and coordinate
  /// reference policy, and chooses plain, optimized single-part, or optimized partitioned
  /// execution. The returned result confirms the sink completed and carries recoverable warnings.
  /// Call [`crate::validate()`] separately when the caller requires a post-write optimized dataset
  /// validation report.
  ///
  /// # Errors
  ///
  /// Returns an error when input format or source metadata cannot be resolved, geometry or its
  /// coordinate reference is missing or unsupported, a reserved internal column exists, output
  /// path topology conflicts with the selected mode, compression is invalid, DataFusion resource
  /// settings are zero, planning or reprojection fails, or the Parquet sink cannot complete.
  /// Failed writes attempt to remove files created by the sink. A cleanup failure is preserved in
  /// the returned error.
  ///
  /// # Panics
  ///
  /// Panics when `OutputOptions` requests an output WKID other than
  /// [`crate::DEFAULT_OUTPUT_WKID`], because additional output references are not implemented.
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

  async fn execute(self) -> Result<SpatialPipelineResult> {
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
  fn finish(&self, rows_written: u64) -> SpatialPipelineResult {
    SpatialPipelineResult::new(self.total_input_rows, rows_written, &self.warnings)
  }
}

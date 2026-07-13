//! Orchestrates one complete input-to-optimized-GeoParquet execution.
//!
//! [`run_optimize_job`] validates input/output options, opens a format-neutral source, configures
//! DataFusion, resolves geometry and CRS metadata, analyzes the output extent, generates spatial
//! helper expressions, and writes GeoParquet plus geodisplay metadata. Point features receive
//! coordinate columns and Morton Z order. Non-point features receive transformed bounds, XZ order,
//! and multiscale PBF payloads. Optional covering output adds the GeoParquet 1.1 bbox structure.
//!
//! The pipeline can execute the source more than once. Missing or invalidated metadata requires an
//! analysis plan. Multi-file output adds a percentile aggregate that estimates balanced spatial
//! code ranges. The final plan repeats source reading, applies UDFs, sorts or repartitions rows,
//! compresses them, and commits output. Small bounded HTTP ranges can be materialized to avoid
//! repeated remote scans.
//!
//! Single-file output uses DataFusion's copy-to plan through a tracking Parquet format. Multi-file
//! output rewrites the physical plan to hash-partition range identifiers, preserve a spatial sort
//! inside each partition, and execute partition writes concurrently. Custom sink wrappers expose
//! live row metrics and remove partially written objects after failures. Explain mode prints
//! logical plans, physical plans, spill activity, operator hotspots, and stage timings.

use std::any::Any;
use std::collections::HashMap;
use std::fmt;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::{
  Arc,
  atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use arrow_array::{Array, Float64Array, RecordBatch, UInt64Array};
use arrow_schema::{DataType, Field, Schema, SchemaRef, SortOptions};
use async_trait::async_trait;
use datafusion::catalog::Session;
use datafusion::common::{
  DataFusionError, GetExt, Result as DataFusionResult, Statistics,
  config::{ConfigField, TableParquetOptions},
  format::{ExplainAnalyzeLevel, ExplainFormat},
};
use datafusion::datasource::file_format::{
  FileFormat, FileFormatFactory,
  file_compression_type::FileCompressionType,
  format_as_file_type,
  parquet::{ParquetFormat, ParquetSink},
};
use datafusion::datasource::listing::ListingTableUrl;
use datafusion::datasource::physical_plan::{FileSinkConfig, FileSource};
use datafusion::datasource::sink::{DataSink, DataSinkExec};
use datafusion::datasource::table_schema::TableSchema;
use datafusion::execution::TaskContext;
use datafusion::execution::context::SessionState;
use datafusion::functions::core::expr_ext::FieldAccessor;
use datafusion::functions_aggregate::approx_percentile_cont::approx_percentile_cont;
use datafusion::functions_aggregate::expr_fn::{max, min};
use datafusion::logical_expr::expr_fn::ident;
use datafusion::logical_expr::{
  ExplainOption, Expr, LogicalPlan, LogicalPlanBuilder, SortExpr, dml::InsertOp, when,
};
use datafusion::physical_expr::expressions::Column as PhysicalColumn;
use datafusion::physical_expr::{
  Distribution, EquivalenceProperties, LexOrdering, LexRequirement, PhysicalSortExpr,
};
use datafusion::physical_plan::{
  DisplayAs, DisplayFormatType, ExecutionPlan, ExecutionPlanProperties, Partitioning,
  PlanProperties, SendableRecordBatchStream, collect, displayable, execute_input_stream,
  execute_stream,
  execution_plan::{EvaluationType, SchedulingType},
  metrics::{Count, ExecutionPlanMetricsSet, MetricBuilder, MetricType, MetricsSet},
  projection::{ProjectionExec, ProjectionExpr},
  repartition::RepartitionExec,
  sorts::sort::SortExec,
  stream::RecordBatchStreamAdapter,
};
use datafusion::prelude::lit;
use engine::plan::{OutputPlan, output_paths, target_rows_per_file, validate_output};
use engine::session::{DataFusionSession, new_datafusion_session};
use engine::write::{
  create_datafusion_parquet_options, create_output_writer, finalize_writers, parse_compression,
  write_batches,
};
use futures_util::StreamExt;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use object_store::{ObjectMeta, ObjectStore};
use parquet::file::metadata::KeyValue;
use serde_json::Value;
use tokio::task::JoinSet;

use crate::analysis::{
  DisplayGeometryType, DisplayJobAnalysis, GeometryFamily, source_display_geometry_type,
};
use crate::codes::DEFAULT_COORDINATE_PRECISION;
use crate::display::{
  BOUNDS_COLUMN, COVERING_BBOX_COLUMN, DISPLAY_COLUMN, POINT_X_COLUMN, POINT_Y_COLUMN,
  POINT_Z_CODE_COLUMN, TEMP_BOUNDS_COLUMN, TEMP_POINT_COORDS_COLUMN,
  TEMP_REPROJECTED_GEOMETRY_COLUMN, TEMP_XMAX_COLUMN, TEMP_XMIN_COLUMN, TEMP_XZ_CODE_COLUMN,
  TEMP_YMAX_COLUMN, TEMP_YMIN_COLUMN, XZ_CODE_COLUMN,
};
use crate::geometry::{GeometryEncoding, GeometryKind, GeometrySpec};
use crate::input::gpkg::GpkgInputProvider;
use crate::input::parquet::ParquetInputProvider;
use crate::input::{
  InputOpenOptions, InputProvider, InputSource, RowRange, is_http_url, open_input,
};
use crate::metadata::output::{DisplayIndexXz, DisplayIndexZ, GeodisplayMetadata};
use crate::metadata::source::SourceDatasetMetadata;
use crate::multiscale::{
  DEFAULT_MAX_LEVEL, DISPLAY_OUTPUT_WKID, create_geometry_encodings, metadata_levels,
};
use crate::reprojection::ReprojectionPlan;
use crate::udf::{
  bounds_xmax_expr, bounds_xmin_expr, bounds_ymax_expr, bounds_ymin_expr, feature_bbox_expr,
  non_point_geodisplay_expr, non_point_xzcode_from_bounds_expr, point_x_expr, point_y_expr,
  point_zcode_from_xy_expr, register_display_udfs, reproject_geometry_expr,
  transformed_bounds_expr, transformed_point_coords_expr,
};

const POINT_RANGE_COLUMN: &str = "z_order";
const NON_POINT_RANGE_COLUMN: &str = "xz_order";
const SINK_ROWS_METRIC: &str = "sink_rows";
const MAX_HTTP_RANGE_ROWS: usize = 100;

/// Configures one complete optimization or pass-through execution.
pub struct OptimizeJobOptions {
  /// Stores the local path or HTTP URL to read.
  pub input: String,
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
  /// Enables a GeoParquet 1.1 covering bbox column.
  pub covering: bool,
  /// Allows replacement of a compatible existing output.
  pub overwrite: bool,
  /// Enables interactive progress reporting.
  pub progress: bool,
  /// Enables verbose DataFusion plans, metrics, and timing diagnostics.
  pub explain: bool,
  /// Bypasses spatial analysis, generated columns, sorting, and metadata changes.
  pub no_optimization: bool,
}

/// Owns validated source, destination, and execution resources for one job.
struct OpenedOptimizeJob {
  input: Arc<dyn InputSource>,
  output_plan: OutputPlan,
  source_schema: SchemaRef,
  total_input_rows: u64,
  session: DataFusionSession,
}

/// Stores spatial metadata and transform decisions shared by optimization stages.
struct SpatialPlanningContext {
  source_metadata: SourceDatasetMetadata,
  geometry_spec: GeometrySpec,
  reprojection: ReprojectionPlan,
  geometry_type: DisplayGeometryType,
}

/// Carries the final DataFrame and physical output controls into the writer stage.
struct PreparedOptimizeOutput {
  dataframe: engine::DataFrame,
  analysis: DisplayJobAnalysis,
  kv_metadata: Vec<KeyValue>,
  partition_column: Option<&'static str>,
  retained_sort_column: Option<&'static str>,
}

/// Execute one spatial optimization job from provider selection through durable output.
///
/// Metadata can remove the analysis scan. Multi-file output adds a percentile scan to
/// estimate spatial range boundaries before the final transform and write scan.
pub async fn run_optimize_job(options: OptimizeJobOptions) -> Result<()> {
  let job_start = Instant::now();
  let job = open_optimize_job(&options).await?;
  let materialized_row_range = materialize_selected_http_range(&job, &options).await?;

  if options.no_optimization {
    let rows_written = run_passthrough_job(
      job.input.as_ref(),
      job.session.context(),
      &job.output_plan,
      &options,
      job.total_input_rows,
      materialized_row_range.as_deref(),
    )
    .await?;
    explain_stage_note(
      options.explain,
      "Pass-through",
      &format!("wrote {rows_written} selected rows without display optimization"),
    );
  } else {
    let planning = build_spatial_planning_context(&job, &options)?;
    let prepared =
      prepare_optimized_output(&job, &options, &planning, materialized_row_range.as_deref())
        .await?;
    write_optimized_output(&job, &options, prepared).await?;
  }

  report_job_completion(&options, job_start);
  Ok(())
}

/// Open and validate source, destination, and DataFusion resources for one job.
async fn open_optimize_job(options: &OptimizeJobOptions) -> Result<OpenedOptimizeJob> {
  validate_http_row_range(&options.input, options.row_range)?;
  let providers: Vec<Box<dyn InputProvider>> = vec![
    Box::new(ParquetInputProvider::new()),
    Box::new(GpkgInputProvider::new()),
  ];
  let input = open_input(
    &InputOpenOptions {
      location: options.input.clone(),
      layer: options.layer.clone(),
    },
    &providers,
  )
  .await?;
  let output_plan = validate_output(&options.output, options.output_files, options.overwrite)?;
  let source_schema = input.schema()?;
  validate_covering_configuration(
    options.covering,
    options.no_optimization,
    source_schema.as_ref(),
  )?;
  let discovered_rows = input.total_rows()?;
  let total_input_rows = options.row_range.effective_rows(discovered_rows);
  explain_run_configuration(
    options.explain,
    input.format_name(),
    options,
    discovered_rows,
    total_input_rows,
    output_plan.parts,
  );

  let session = new_datafusion_session()?;
  configure_explain_session(session.context(), options.explain);
  register_display_udfs(session.context());
  Ok(OpenedOptimizeJob {
    input,
    output_plan,
    source_schema,
    total_input_rows,
    session,
  })
}

/// Materialize a small selected HTTP range once for reuse by independent job plans.
async fn materialize_selected_http_range(
  job: &OpenedOptimizeJob,
  options: &OptimizeJobOptions,
) -> Result<Option<Vec<RecordBatch>>> {
  if !should_materialize_bounded_http_range(job.input.as_ref(), options.row_range) {
    return Ok(None);
  }

  let range_bar = row_bar(
    options.progress,
    "Reading selected row range",
    job.total_input_rows,
  );
  let range_start = Instant::now();
  let batches =
    materialize_input_row_range(job.input.as_ref(), options.row_range, &range_bar).await?;
  finish_row_bar(
    &range_bar,
    job.total_input_rows,
    "Read selected row range".to_string(),
  );
  explain_timing(
    options.explain,
    "Reading selected row range",
    range_start.elapsed(),
  );
  Ok(Some(batches))
}

/// Resolve source geometry, CRS, and reprojection decisions before executing analysis.
fn build_spatial_planning_context(
  job: &OpenedOptimizeJob,
  options: &OptimizeJobOptions,
) -> Result<SpatialPlanningContext> {
  let source_metadata = job.input.source_metadata()?;
  let geometry_spec = resolve_input_geometry_spec(
    job.source_schema.as_ref(),
    job.input.inferred_geometry_spec()?,
    options.geometry_column.as_deref(),
  )?;
  let reprojection = ReprojectionPlan::from_source_metadata(
    &source_metadata,
    &geometry_spec.column,
    DISPLAY_OUTPUT_WKID,
  )?;
  let geometry_type = source_display_geometry_type(&source_metadata, &geometry_spec.column)
    .with_context(|| {
      format!(
        "unable to determine display geometry type for column '{}' from source metadata; \
         explicit geometry type metadata is required",
        geometry_spec.column
      )
    })?;
  Ok(SpatialPlanningContext {
    source_metadata,
    geometry_spec,
    reprojection,
    geometry_type,
  })
}

/// Analyze geometry, construct helper expressions, and build the final output DataFrame.
async fn prepare_optimized_output(
  job: &OpenedOptimizeJob,
  options: &OptimizeJobOptions,
  planning: &SpatialPlanningContext,
  materialized_batches: Option<&[RecordBatch]>,
) -> Result<PreparedOptimizeOutput> {
  let analysis = analyze_optimized_geometry(job, options, planning, materialized_batches).await?;
  ensure_supported(&analysis)?;

  let encodings = match analysis.geometry_family {
    GeometryFamily::Point => Vec::new(),
    GeometryFamily::NonPoint => {
      create_geometry_encodings(DISPLAY_OUTPUT_WKID, analysis.geometry_type)?
    }
  };
  let partition_column = (job.output_plan.parts > 1).then_some(partition_column_name(&analysis));
  validate_partition_column(job.source_schema.as_ref(), partition_column)?;
  let prepared_df =
    prepare_spatially_ordered_dataframe(job, options, planning, &analysis, materialized_batches)
      .await?;
  let retained_sort_column =
    if partition_column.is_some() && matches!(analysis.geometry_family, GeometryFamily::NonPoint) {
      Some(sort_column_name(&analysis))
    } else {
      None
    };
  let dataframe = prepared_df.select(build_final_projection_exprs(
    job.source_schema.as_ref(),
    &analysis,
    &encodings,
    partition_column,
    retained_sort_column,
    planning
      .reprojection
      .requires_reprojection()
      .then_some(TEMP_REPROJECTED_GEOMETRY_COLUMN),
    options.covering,
  ))?;
  let kv_metadata = build_output_metadata(
    &planning.source_metadata,
    &analysis,
    &encodings,
    options.covering,
  )?;

  Ok(PreparedOptimizeOutput {
    dataframe,
    analysis,
    kv_metadata,
    partition_column,
    retained_sort_column,
  })
}

/// Analyze geometry from complete metadata or an executed helper DataFrame.
async fn analyze_optimized_geometry(
  job: &OpenedOptimizeJob,
  options: &OptimizeJobOptions,
  planning: &SpatialPlanningContext,
  materialized_batches: Option<&[RecordBatch]>,
) -> Result<DisplayJobAnalysis> {
  let analysis_bar = row_bar(options.progress, "Analyzing geometry", job.total_input_rows);
  let analysis = if let Some(analysis) = metadata_display_analysis(
    &planning.geometry_spec,
    &planning.source_metadata,
    planning.geometry_type,
    planning.reprojection.target_spatial_reference().clone(),
    options.row_range.is_full() && !planning.reprojection.requires_reprojection(),
  ) {
    explain_stage_note(
      options.explain,
      "Analyzing geometry",
      "using metadata fast path from source metadata",
    );
    explain_timing(options.explain, "Analyzing geometry", Duration::ZERO);
    analysis_bar.inc(job.total_input_rows);
    analysis
  } else if job.output_plan.parts > 1 {
    let helper_df = build_narrow_helper_projection_df(
      input_dataframe_for_job(
        job.input.as_ref(),
        job.session.context(),
        options.row_range,
        materialized_batches,
      )
      .await?,
      &planning.geometry_spec,
      planning.geometry_type,
      planning.reprojection.transform(),
    )?;
    analyze_base_helper_df_with_metric_polling(
      helper_df,
      &planning.geometry_spec,
      &planning.source_metadata,
      planning.geometry_type,
      planning.reprojection.target_spatial_reference().clone(),
      &analysis_bar,
      job.total_input_rows,
      options.explain,
    )
    .await?
  } else {
    let df = input_dataframe_for_job(
      job.input.as_ref(),
      job.session.context(),
      options.row_range,
      materialized_batches,
    )
    .await?;
    analyze_display_df_with_metric_polling(
      df,
      &planning.geometry_spec,
      &planning.source_metadata,
      planning.geometry_type,
      planning.reprojection.transform(),
      planning.reprojection.target_spatial_reference().clone(),
      &analysis_bar,
      job.total_input_rows,
      options.explain,
    )
    .await?
  };
  finish_row_bar(
    &analysis_bar,
    job.total_input_rows,
    format!("Analyzed {} geometry", analysis.geometry_type.as_str()),
  );
  Ok(analysis)
}

/// Reject an internal partition column that would overwrite source data.
fn validate_partition_column(
  source_schema: &arrow_schema::Schema,
  partition_column: Option<&str>,
) -> Result<()> {
  if let Some(partition_column) = partition_column
    && source_schema.field_with_name(partition_column).is_ok()
  {
    bail!("output partition column '{partition_column}' conflicts with an existing input column");
  }
  Ok(())
}

/// Build either a globally sorted DataFrame or a range-partitioned multi-file DataFrame.
async fn prepare_spatially_ordered_dataframe(
  job: &OpenedOptimizeJob,
  options: &OptimizeJobOptions,
  planning: &SpatialPlanningContext,
  analysis: &DisplayJobAnalysis,
  materialized_batches: Option<&[RecordBatch]>,
) -> Result<engine::DataFrame> {
  if job.output_plan.parts > 1 {
    let partition_column = partition_column_name(analysis);
    let range_bar = row_bar(
      options.progress,
      "Computing partition ranges",
      job.total_input_rows,
    );
    let range_source_df = add_sort_columns_df(
      build_narrow_helper_projection_df(
        input_dataframe_for_job(
          job.input.as_ref(),
          job.session.context(),
          options.row_range,
          materialized_batches,
        )
        .await?,
        &planning.geometry_spec,
        planning.geometry_type,
        planning.reprojection.transform(),
      )?,
      analysis,
    )?;
    let boundaries = compute_range_partition_boundaries(
      range_source_df,
      sort_column_name(analysis),
      job.output_plan.parts,
      &range_bar,
      job.total_input_rows,
      options.explain,
    )
    .await?;
    finish_row_bar(
      &range_bar,
      job.total_input_rows,
      "Computed partition ranges".to_string(),
    );
    return Ok(
      build_helper_projection_df(
        input_dataframe_for_job(
          job.input.as_ref(),
          job.session.context(),
          options.row_range,
          materialized_batches,
        )
        .await?,
        job.source_schema.as_ref(),
        analysis,
        planning.reprojection.transform(),
      )?
      .with_column(
        partition_column,
        build_range_partition_expr(
          sort_column_name(analysis),
          boundaries.min_value,
          &boundaries.boundaries,
        )?,
      )?,
    );
  }

  build_helper_projection_df(
    input_dataframe_for_job(
      job.input.as_ref(),
      job.session.context(),
      options.row_range,
      materialized_batches,
    )
    .await?,
    job.source_schema.as_ref(),
    analysis,
    planning.reprojection.transform(),
  )?
  .sort(vec![sort_expr(analysis)])
  .map_err(Into::into)
}

/// Configure the final Parquet sink and execute the prepared output DataFrame.
async fn write_optimized_output(
  job: &OpenedOptimizeJob,
  options: &OptimizeJobOptions,
  prepared: PreparedOptimizeOutput,
) -> Result<u64> {
  let compression = parse_compression(options.compression.as_deref().unwrap_or("snappy"))?;
  let writer_options = create_datafusion_parquet_options(compression, &prepared.kv_metadata);
  let multi_file_output = job.output_plan.parts > 1;
  let write_bar = row_bar(
    options.progress,
    write_stage_message(WriteStagePhase::Reading, multi_file_output),
    job.total_input_rows,
  );
  let (write_path, partition_by, partitioned_write) = if multi_file_output {
    let partition_column = prepared
      .partition_column
      .context("partition column should exist for multi-file output")?;
    (
      job.output_plan.path.to_string_lossy().into_owned(),
      vec![partition_column.to_string()],
      Some(PartitionedWriteConfig {
        partition_column: partition_column.to_string(),
        sort_column: sort_column_name(&prepared.analysis).to_string(),
        bucket_count: job.output_plan.parts,
        drop_sort_column_after_sort: prepared.retained_sort_column.is_some(),
      }),
    )
  } else {
    (
      output_paths(&job.output_plan)?
        .into_iter()
        .next()
        .context("missing output path")?
        .to_string_lossy()
        .into_owned(),
      Vec::new(),
      None,
    )
  };
  let rows_written = write_parquet_with_metric_polling(
    prepared.dataframe,
    &write_path,
    partition_by,
    partitioned_write,
    writer_options,
    &write_bar,
    job.total_input_rows,
    options.explain,
  )
  .await?;
  finish_spinner(
    &write_bar,
    format!("Completed write pipeline ({rows_written} rows)"),
  );
  Ok(rows_written)
}

/// Report terminal timing after either pass-through or optimized execution.
fn report_job_completion(options: &OptimizeJobOptions, job_start: Instant) {
  if options.progress && std::io::stderr().is_terminal() {
    eprintln!("Completed in {}", format_elapsed(job_start.elapsed()));
  }
  explain_timing(options.explain, "Total job", job_start.elapsed());
}

/// Build analysis from source metadata when it describes the exact output coordinate space.
fn metadata_display_analysis(
  geometry_spec: &GeometrySpec,
  source_metadata: &SourceDatasetMetadata,
  geometry_type: DisplayGeometryType,
  spatial_reference: crate::analysis::SpatialReferenceInfo,
  allow_fast_path: bool,
) -> Option<DisplayJobAnalysis> {
  if !allow_fast_path {
    return None;
  }
  let source_geometry = source_metadata
    .geometry
    .as_ref()
    .filter(|geometry| geometry.column == geometry_spec.column)?;
  let full_extent = source_geometry.bbox?;
  Some(DisplayJobAnalysis {
    geometry_spec: geometry_spec.clone(),
    geometry_type,
    geometry_family: geometry_type.family(),
    full_extent,
    spatial_reference,
    has_z: source_geometry.has_z,
    has_m: source_geometry.has_m,
  })
}

/// Write selected input rows without generated display columns or spatial ordering.
async fn run_passthrough_job(
  input: &dyn InputSource,
  ctx: &engine::SessionContext,
  output_plan: &OutputPlan,
  options: &OptimizeJobOptions,
  total_input_rows: u64,
  materialized_batches: Option<&[RecordBatch]>,
) -> Result<u64> {
  if output_plan.parts != 1 {
    bail!("--no-optimiztaion does not support --output-files");
  }
  let df = input_dataframe_for_job(input, ctx, options.row_range, materialized_batches).await?;
  let kv_metadata = input.file_metadata()?;
  let compression = parse_compression(options.compression.as_deref().unwrap_or("snappy"))?;
  let writer_options = create_datafusion_parquet_options(compression, &kv_metadata);
  let write_path = output_paths(output_plan)?
    .into_iter()
    .next()
    .context("missing output path")?
    .to_string_lossy()
    .into_owned();
  let write_bar = row_bar(
    options.progress,
    "Writing pass-through parquet output",
    total_input_rows,
  );
  let rows_written = write_parquet_with_metric_polling(
    df,
    &write_path,
    Vec::new(),
    None,
    writer_options,
    &write_bar,
    total_input_rows,
    options.explain,
  )
  .await?;
  finish_spinner(
    &write_bar,
    format!("Completed pass-through write ({rows_written} rows)"),
  );
  Ok(rows_written)
}

/// Decide whether to cache a selected HTTP range before building DataFusion plans.
///
/// Analysis, range estimation, and writing can execute independent source scans. Materializing
/// at most [`MAX_HTTP_RANGE_ROWS`] rows converts those repeated network range requests into
/// in-memory batch scans while keeping memory use explicitly bounded.
fn should_materialize_bounded_http_range(input: &dyn InputSource, row_range: RowRange) -> bool {
  matches!(row_range.num, Some(num) if num <= MAX_HTTP_RANGE_ROWS)
    && !row_range.is_full()
    && is_http_url(input.source_location())
}

/// Validate bounded-read requirements for direct HTTP Parquet input.
///
/// HTTP offsets must include a small limit because the materialization path intentionally
/// avoids unbounded downloads and repeated remote scans.
fn validate_http_row_range(input_location: &str, row_range: RowRange) -> Result<()> {
  if !is_http_url(input_location) {
    return Ok(());
  }
  if row_range.start > 0 && row_range.num.is_none() {
    bail!("HTTP parquet input with --start requires --num (maximum {MAX_HTTP_RANGE_ROWS} rows)");
  }
  if let Some(num) = row_range.num
    && num > MAX_HTTP_RANGE_ROWS
  {
    bail!("HTTP parquet input supports at most --num {MAX_HTTP_RANGE_ROWS} for ranged reads");
  }
  Ok(())
}

/// Materialize a small HTTP row range once to avoid repeated remote scans.
async fn materialize_input_row_range(
  input: &dyn InputSource,
  row_range: RowRange,
  progress_bar: &ProgressBar,
) -> Result<Vec<RecordBatch>> {
  let mut batches = Vec::new();
  let mut stream = input.read_batches(row_range).await?;
  while let Some(batch) = stream.next().await {
    let batch = batch?;
    progress_bar.inc(batch.num_rows() as u64);
    batches.push(batch);
  }
  if batches.is_empty() {
    batches.push(RecordBatch::new_empty(input.schema()?));
  }
  Ok(batches)
}

/// Build a source DataFrame or reuse previously materialized batches.
async fn input_dataframe_for_job(
  input: &dyn InputSource,
  ctx: &engine::SessionContext,
  row_range: RowRange,
  materialized_batches: Option<&[RecordBatch]>,
) -> Result<engine::DataFrame> {
  if let Some(batches) = materialized_batches {
    return Ok(ctx.read_batches(batches.iter().cloned())?);
  }
  input.to_dataframe(ctx, row_range).await
}

/// Build full-width helper columns while preserving every source field.
fn build_base_helper_projection_df(
  df: engine::DataFrame,
  source_schema: &arrow_schema::Schema,
  geometry_spec: &GeometrySpec,
  geometry_type: DisplayGeometryType,
  transform: Option<&crate::reprojection::TransformSpec>,
) -> Result<engine::DataFrame> {
  let projected = df.select(
    source_schema
      .fields()
      .iter()
      .map(|field| ident(field.name()))
      .collect::<Vec<_>>(),
  )?;
  add_geometry_helper_columns_df(projected, geometry_spec, geometry_type, transform)
}

/// Build a geometry-only helper projection for analysis and range estimation.
fn build_narrow_helper_projection_df(
  df: engine::DataFrame,
  geometry_spec: &GeometrySpec,
  geometry_type: DisplayGeometryType,
  transform: Option<&crate::reprojection::TransformSpec>,
) -> Result<engine::DataFrame> {
  let projected = df.select(vec![ident(&geometry_spec.column)])?;
  add_geometry_helper_columns_df(projected, geometry_spec, geometry_type, transform)
}

/// Add reprojection and point-coordinate or non-point-bound helper columns.
fn add_geometry_helper_columns_df(
  mut projected: engine::DataFrame,
  geometry_spec: &GeometrySpec,
  geometry_type: DisplayGeometryType,
  transform: Option<&crate::reprojection::TransformSpec>,
) -> Result<engine::DataFrame> {
  let geometry_column = if let Some(transform) = transform {
    projected = projected.with_column(
      TEMP_REPROJECTED_GEOMETRY_COLUMN,
      reproject_geometry_expr(&geometry_spec.column, transform),
    )?;
    TEMP_REPROJECTED_GEOMETRY_COLUMN
  } else {
    &geometry_spec.column
  };
  match geometry_type.family() {
    GeometryFamily::Point => {
      projected = projected.with_column(POINT_X_COLUMN, point_x_expr(geometry_column))?;
      projected = projected.with_column(POINT_Y_COLUMN, point_y_expr(geometry_column))?;
    }
    GeometryFamily::NonPoint => {
      projected = projected.with_column(TEMP_XMIN_COLUMN, bounds_xmin_expr(geometry_column))?;
      projected = projected.with_column(TEMP_YMIN_COLUMN, bounds_ymin_expr(geometry_column))?;
      projected = projected.with_column(TEMP_XMAX_COLUMN, bounds_xmax_expr(geometry_column))?;
      projected = projected.with_column(TEMP_YMAX_COLUMN, bounds_ymax_expr(geometry_column))?;
    }
  }
  Ok(projected)
}

/// Add the Z-order or XZ-order column used for sorting and range partitioning.
fn add_sort_columns_df(
  df: engine::DataFrame,
  analysis: &DisplayJobAnalysis,
) -> Result<engine::DataFrame> {
  match analysis.geometry_family {
    GeometryFamily::Point => Ok(df.with_column(
      POINT_Z_CODE_COLUMN,
      point_zcode_from_xy_expr(POINT_X_COLUMN, POINT_Y_COLUMN, analysis.full_extent),
    )?),
    GeometryFamily::NonPoint => Ok(df.with_column(
      TEMP_XZ_CODE_COLUMN,
      non_point_xzcode_from_bounds_expr(
        TEMP_XMIN_COLUMN,
        TEMP_YMIN_COLUMN,
        TEMP_XMAX_COLUMN,
        TEMP_YMAX_COLUMN,
        analysis.full_extent,
      ),
    )?),
  }
}

/// Compose source, helper, sort, reprojection, and optional covering expressions.
fn build_helper_projection_df(
  df: engine::DataFrame,
  source_schema: &arrow_schema::Schema,
  analysis: &DisplayJobAnalysis,
  transform: Option<&crate::reprojection::TransformSpec>,
) -> Result<engine::DataFrame> {
  let projected = build_base_helper_projection_df(
    df,
    source_schema,
    &analysis.geometry_spec,
    analysis.geometry_type,
    transform,
  )?;
  add_sort_columns_df(projected, analysis)
}

/// Build the final public projection and remove temporary planning columns.
fn build_final_projection_exprs(
  source_schema: &arrow_schema::Schema,
  analysis: &DisplayJobAnalysis,
  encodings: &[crate::multiscale::GeometryEncoding],
  partition_column: Option<&str>,
  retained_sort_column: Option<&str>,
  projected_geometry_column: Option<&str>,
  covering: bool,
) -> Vec<Expr> {
  let mut exprs = source_schema
    .fields()
    .iter()
    .filter(|field| !is_generated_display_output_column(field.name(), analysis.geometry_family))
    .map(|field| {
      if Some(field.name().as_str()) == Some(analysis.geometry_spec.column.as_str()) {
        if let Some(projected_geometry_column) = projected_geometry_column {
          return ident(projected_geometry_column).alias(field.name());
        }
      }
      ident(field.name())
    })
    .collect::<Vec<_>>();
  if covering {
    let geometry_column = projected_geometry_column.unwrap_or(&analysis.geometry_spec.column);
    match analysis.geometry_family {
      GeometryFamily::Point => exprs.push(feature_bbox_expr(
        geometry_column,
        POINT_X_COLUMN,
        POINT_Y_COLUMN,
        POINT_X_COLUMN,
        POINT_Y_COLUMN,
      )),
      GeometryFamily::NonPoint => exprs.push(feature_bbox_expr(
        geometry_column,
        TEMP_XMIN_COLUMN,
        TEMP_YMIN_COLUMN,
        TEMP_XMAX_COLUMN,
        TEMP_YMAX_COLUMN,
      )),
    }
  }
  match analysis.geometry_family {
    GeometryFamily::Point => {
      exprs.push(ident(POINT_Z_CODE_COLUMN));
      exprs.push(ident(POINT_X_COLUMN));
      exprs.push(ident(POINT_Y_COLUMN));
    }
    GeometryFamily::NonPoint => exprs.push(non_point_geodisplay_expr(
      projected_geometry_column.unwrap_or(&analysis.geometry_spec.column),
      analysis.geometry_type,
      encodings,
    )),
  }
  if let Some(partition_column) = partition_column {
    exprs.push(ident(partition_column));
  }
  if let Some(retained_sort_column) = retained_sort_column {
    exprs.push(ident(retained_sort_column));
  }
  exprs
}

/// Return whether a source column should be replaced by newly generated display output.
fn is_generated_display_output_column(name: &str, geometry_family: GeometryFamily) -> bool {
  match geometry_family {
    GeometryFamily::Point => {
      matches!(name, POINT_Z_CODE_COLUMN | POINT_X_COLUMN | POINT_Y_COLUMN)
    }
    GeometryFamily::NonPoint => name == DISPLAY_COLUMN,
  }
}

/// Analyze a source DataFrame by decoding geometry expressions and collecting aggregates.
async fn analyze_display_df_with_metric_polling(
  df: engine::DataFrame,
  geometry_spec: &GeometrySpec,
  source_metadata: &SourceDatasetMetadata,
  geometry_type: DisplayGeometryType,
  transform: Option<&crate::reprojection::TransformSpec>,
  output_spatial_reference: crate::analysis::SpatialReferenceInfo,
  progress_bar: &ProgressBar,
  total_input_rows: u64,
  explain: bool,
) -> Result<DisplayJobAnalysis> {
  let aggregate_df = match (geometry_type.family(), transform) {
    (GeometryFamily::Point, Some(transform)) => df
      .with_column(
        TEMP_POINT_COORDS_COLUMN,
        transformed_point_coords_expr(&geometry_spec.column, transform),
      )?
      .select(vec![
        point_coords_field_expr("x").alias(POINT_X_COLUMN),
        point_coords_field_expr("y").alias(POINT_Y_COLUMN),
      ])?
      .aggregate(
        vec![],
        vec![
          min(ident(POINT_X_COLUMN)).alias(TEMP_XMIN_COLUMN),
          min(ident(POINT_Y_COLUMN)).alias(TEMP_YMIN_COLUMN),
          max(ident(POINT_X_COLUMN)).alias(TEMP_XMAX_COLUMN),
          max(ident(POINT_Y_COLUMN)).alias(TEMP_YMAX_COLUMN),
        ],
      )?,
    (GeometryFamily::Point, None) => df
      .select(vec![
        point_x_expr(&geometry_spec.column),
        point_y_expr(&geometry_spec.column),
      ])?
      .aggregate(
        vec![],
        vec![
          min(ident(POINT_X_COLUMN)).alias(TEMP_XMIN_COLUMN),
          min(ident(POINT_Y_COLUMN)).alias(TEMP_YMIN_COLUMN),
          max(ident(POINT_X_COLUMN)).alias(TEMP_XMAX_COLUMN),
          max(ident(POINT_Y_COLUMN)).alias(TEMP_YMAX_COLUMN),
        ],
      )?,
    (GeometryFamily::NonPoint, Some(transform)) => df
      .with_column(
        TEMP_BOUNDS_COLUMN,
        transformed_bounds_expr(&geometry_spec.column, geometry_type, transform),
      )?
      .select(vec![
        bounds_field_expr("xmin").alias(TEMP_XMIN_COLUMN),
        bounds_field_expr("ymin").alias(TEMP_YMIN_COLUMN),
        bounds_field_expr("xmax").alias(TEMP_XMAX_COLUMN),
        bounds_field_expr("ymax").alias(TEMP_YMAX_COLUMN),
      ])?
      .aggregate(
        vec![],
        vec![
          min(ident(TEMP_XMIN_COLUMN)).alias(TEMP_XMIN_COLUMN),
          min(ident(TEMP_YMIN_COLUMN)).alias(TEMP_YMIN_COLUMN),
          max(ident(TEMP_XMAX_COLUMN)).alias(TEMP_XMAX_COLUMN),
          max(ident(TEMP_YMAX_COLUMN)).alias(TEMP_YMAX_COLUMN),
        ],
      )?,
    (GeometryFamily::NonPoint, None) => df
      .select(vec![
        bounds_xmin_expr(&geometry_spec.column),
        bounds_ymin_expr(&geometry_spec.column),
        bounds_xmax_expr(&geometry_spec.column),
        bounds_ymax_expr(&geometry_spec.column),
      ])?
      .aggregate(
        vec![],
        vec![
          min(ident(TEMP_XMIN_COLUMN)).alias(TEMP_XMIN_COLUMN),
          min(ident(TEMP_YMIN_COLUMN)).alias(TEMP_YMIN_COLUMN),
          max(ident(TEMP_XMAX_COLUMN)).alias(TEMP_XMAX_COLUMN),
          max(ident(TEMP_YMAX_COLUMN)).alias(TEMP_YMAX_COLUMN),
        ],
      )?,
  };
  let batches = collect_df_with_metric_polling(
    aggregate_df,
    progress_bar,
    total_input_rows,
    "Analyzing geometry",
    explain,
  )
  .await?;
  let full_extent = extract_extent_from_aggregate_batches(&batches)?;
  let (has_z, has_m) = source_metadata
    .geometry
    .as_ref()
    .filter(|geometry| geometry.column == geometry_spec.column)
    .map(|geometry| (geometry.has_z, geometry.has_m))
    .unwrap_or((false, false));
  Ok(DisplayJobAnalysis {
    geometry_spec: geometry_spec.clone(),
    geometry_type,
    geometry_family: geometry_type.family(),
    full_extent,
    spatial_reference: output_spatial_reference,
    has_z,
    has_m,
  })
}

/// Analyze a helper DataFrame whose transformed coordinates or bounds already exist.
async fn analyze_base_helper_df_with_metric_polling(
  df: engine::DataFrame,
  geometry_spec: &GeometrySpec,
  source_metadata: &SourceDatasetMetadata,
  geometry_type: DisplayGeometryType,
  output_spatial_reference: crate::analysis::SpatialReferenceInfo,
  progress_bar: &ProgressBar,
  total_input_rows: u64,
  explain: bool,
) -> Result<DisplayJobAnalysis> {
  let aggregate_df = match geometry_type.family() {
    GeometryFamily::Point => df.aggregate(
      vec![],
      vec![
        min(ident(POINT_X_COLUMN)).alias(TEMP_XMIN_COLUMN),
        min(ident(POINT_Y_COLUMN)).alias(TEMP_YMIN_COLUMN),
        max(ident(POINT_X_COLUMN)).alias(TEMP_XMAX_COLUMN),
        max(ident(POINT_Y_COLUMN)).alias(TEMP_YMAX_COLUMN),
      ],
    )?,
    GeometryFamily::NonPoint => df.aggregate(
      vec![],
      vec![
        min(ident(TEMP_XMIN_COLUMN)).alias(TEMP_XMIN_COLUMN),
        min(ident(TEMP_YMIN_COLUMN)).alias(TEMP_YMIN_COLUMN),
        max(ident(TEMP_XMAX_COLUMN)).alias(TEMP_XMAX_COLUMN),
        max(ident(TEMP_YMAX_COLUMN)).alias(TEMP_YMAX_COLUMN),
      ],
    )?,
  };
  let batches = collect_df_with_metric_polling(
    aggregate_df,
    progress_bar,
    total_input_rows,
    "Analyzing geometry",
    explain,
  )
  .await?;
  let full_extent = extract_extent_from_aggregate_batches(&batches)?;
  let (has_z, has_m) = source_metadata
    .geometry
    .as_ref()
    .filter(|geometry| geometry.column == geometry_spec.column)
    .map(|geometry| (geometry.has_z, geometry.has_m))
    .unwrap_or((false, false));
  Ok(DisplayJobAnalysis {
    geometry_spec: geometry_spec.clone(),
    geometry_type,
    geometry_family: geometry_type.family(),
    full_extent,
    spatial_reference: output_spatial_reference,
    has_z,
    has_m,
  })
}

/// Select one field from the temporary transformed point-coordinate struct.
fn point_coords_field_expr(field: &str) -> Expr {
  ident(TEMP_POINT_COORDS_COLUMN).field(field)
}

/// Select one field from the temporary transformed geometry-bounds struct.
fn bounds_field_expr(field: &str) -> Expr {
  ident(TEMP_BOUNDS_COLUMN).field(field)
}

/// Build the ascending, nulls-last spatial ordering required by output.
fn sort_expr(analysis: &DisplayJobAnalysis) -> SortExpr {
  match analysis.geometry_family {
    GeometryFamily::Point => ident(POINT_Z_CODE_COLUMN).sort(true, false),
    GeometryFamily::NonPoint => ident(TEMP_XZ_CODE_COLUMN).sort(true, false),
  }
}

/// Return the physical sort column selected by the geometry family.
fn sort_column_name(analysis: &DisplayJobAnalysis) -> &'static str {
  match analysis.geometry_family {
    GeometryFamily::Point => POINT_Z_CODE_COLUMN,
    GeometryFamily::NonPoint => TEMP_XZ_CODE_COLUMN,
  }
}

/// Derive the internal range-partition column selected by the geometry family.
fn partition_column_name(analysis: &DisplayJobAnalysis) -> &'static str {
  match analysis.geometry_family {
    GeometryFamily::Point => POINT_RANGE_COLUMN,
    GeometryFamily::NonPoint => NON_POINT_RANGE_COLUMN,
  }
}

/// Scale sort parallelism above the requested output bucket count.
#[allow(dead_code)]
fn multi_file_sort_partition_count(bucket_count: usize) -> usize {
  bucket_count.saturating_mul(2).max(8)
}

#[allow(dead_code)]
#[derive(Clone)]
/// Describes the columns and fan-out needed for ordered multi-file writing.
struct PartitionedWriteConfig {
  partition_column: String,
  sort_column: String,
  bucket_count: usize,
  drop_sort_column_after_sort: bool,
}

/// Stores the minimum code and percentile-derived lower boundaries for output ranges.
struct RangePartitionBoundaries {
  min_value: u64,
  boundaries: Vec<u64>,
}

/// Estimate balanced spatial-code ranges with one minimum and approximate percentiles.
async fn compute_range_partition_boundaries(
  df: engine::DataFrame,
  sort_column: &str,
  bucket_count: usize,
  progress_bar: &ProgressBar,
  total_input_rows: u64,
  explain: bool,
) -> Result<RangePartitionBoundaries> {
  if bucket_count <= 1 {
    return Ok(RangePartitionBoundaries {
      min_value: 0,
      boundaries: Vec::new(),
    });
  }

  let mut aggregate_exprs = vec![min(ident(sort_column)).alias("range_min")];
  aggregate_exprs.extend(
    (1..bucket_count)
      .map(|index| {
        approx_percentile_cont(
          ident(sort_column).sort(true, false),
          lit(index as f64 / bucket_count as f64),
          None,
        )
        .alias(format!("range_boundary_{index}"))
      })
      .collect::<Vec<_>>(),
  );
  let batches = collect_df_with_metric_polling(
    df.aggregate(vec![], aggregate_exprs)?,
    progress_bar,
    total_input_rows,
    "Computing partition ranges",
    explain,
  )
  .await?;
  let Some(batch) = batches.first() else {
    return Ok(RangePartitionBoundaries {
      min_value: 0,
      boundaries: Vec::new(),
    });
  };
  if batch.num_rows() == 0 {
    return Ok(RangePartitionBoundaries {
      min_value: 0,
      boundaries: Vec::new(),
    });
  }

  let min_values = batch
    .column(0)
    .as_any()
    .downcast_ref::<UInt64Array>()
    .context("range partition minimum aggregate did not return UInt64")?;
  let min_value = if min_values.is_null(0) {
    0
  } else {
    min_values.value(0)
  };

  let mut boundaries = Vec::with_capacity(batch.num_columns().saturating_sub(1));
  for column in batch.columns().iter().skip(1) {
    let values = column
      .as_any()
      .downcast_ref::<UInt64Array>()
      .context("range boundary aggregate did not return UInt64")?;
    if !values.is_null(0) {
      boundaries.push(values.value(0));
    }
  }
  boundaries.sort_unstable();
  Ok(RangePartitionBoundaries {
    min_value,
    boundaries,
  })
}

/// Build a scalar expression that maps spatial codes to range lower-bound identifiers.
fn build_range_partition_expr(
  sort_column: &str,
  min_value: u64,
  boundaries: &[u64],
) -> Result<Expr> {
  let mut lower_bounds = Vec::with_capacity(boundaries.len() + 1);
  lower_bounds.push(min_value);
  lower_bounds.extend(boundaries.iter().copied());

  let mut range_expr = lit(*lower_bounds.last().unwrap_or(&min_value));
  for (index, boundary) in boundaries.iter().enumerate().rev() {
    range_expr = when(
      ident(sort_column).lt(lit(*boundary)),
      lit(lower_bounds[index]),
    )
    .otherwise(range_expr)?;
  }
  Ok(range_expr)
}

#[derive(Debug, Clone)]
/// Produces Parquet formats whose sinks expose repository-specific progress metrics.
struct TrackingParquetFormatFactory {
  options: TableParquetOptions,
}

impl TrackingParquetFormatFactory {
  /// Store the baseline Parquet options applied to every format instance.
  fn new(options: TableParquetOptions) -> Self {
    Self { options }
  }
}

impl GetExt for TrackingParquetFormatFactory {
  fn get_ext(&self) -> String {
    "parquet".to_string()
  }
}

impl FileFormatFactory for TrackingParquetFormatFactory {
  fn create(
    &self,
    _state: &dyn Session,
    format_options: &HashMap<String, String>,
  ) -> DataFusionResult<Arc<dyn FileFormat>> {
    let mut parquet_options = self.options.clone();
    for (key, value) in format_options {
      parquet_options.set(key, value)?;
    }
    Ok(Arc::new(TrackingParquetFormat::new(parquet_options)))
  }

  fn default(&self) -> Arc<dyn FileFormat> {
    Arc::new(TrackingParquetFormat::new(self.options.clone()))
  }

  fn as_any(&self) -> &dyn Any {
    self
  }
}

#[derive(Debug, Clone)]
/// Wraps DataFusion's Parquet format to inject a tracking sink at physical planning time.
struct TrackingParquetFormat {
  options: TableParquetOptions,
}

impl TrackingParquetFormat {
  /// Store the Parquet options used when constructing DataFusion's inner format.
  fn new(options: TableParquetOptions) -> Self {
    Self { options }
  }

  /// Rebuild DataFusion's Parquet format while retaining the configured writer options.
  fn inner_format(&self) -> ParquetFormat {
    ParquetFormat::default().with_options(self.options.clone())
  }
}

#[async_trait]
impl FileFormat for TrackingParquetFormat {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn get_ext(&self) -> String {
    self.inner_format().get_ext()
  }

  fn get_ext_with_compression(
    &self,
    file_compression_type: &FileCompressionType,
  ) -> DataFusionResult<String> {
    self
      .inner_format()
      .get_ext_with_compression(file_compression_type)
  }

  fn compression_type(&self) -> Option<FileCompressionType> {
    self.inner_format().compression_type()
  }

  async fn infer_schema(
    &self,
    state: &dyn Session,
    store: &Arc<dyn ObjectStore>,
    objects: &[ObjectMeta],
  ) -> DataFusionResult<SchemaRef> {
    self
      .inner_format()
      .infer_schema(state, store, objects)
      .await
  }

  async fn infer_stats(
    &self,
    state: &dyn Session,
    store: &Arc<dyn ObjectStore>,
    table_schema: SchemaRef,
    object: &ObjectMeta,
  ) -> DataFusionResult<Statistics> {
    self
      .inner_format()
      .infer_stats(state, store, table_schema, object)
      .await
  }

  async fn create_physical_plan(
    &self,
    state: &dyn Session,
    conf: datafusion::datasource::physical_plan::FileScanConfig,
  ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
    self.inner_format().create_physical_plan(state, conf).await
  }

  async fn create_writer_physical_plan(
    &self,
    input: Arc<dyn ExecutionPlan>,
    _state: &dyn Session,
    conf: FileSinkConfig,
    order_requirements: Option<LexRequirement>,
  ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
    let sink = Arc::new(TrackingParquetSink::new(conf, self.options.clone()));
    Ok(Arc::new(DataSinkExec::new(input, sink, order_requirements)))
  }

  fn file_source(&self, table_schema: TableSchema) -> Arc<dyn FileSource> {
    self.inner_format().file_source(table_schema)
  }
}

#[derive(Debug)]
/// Wraps DataFusion's Parquet sink with row metrics and failed-write cleanup support.
struct TrackingParquetSink {
  config: FileSinkConfig,
  // Wrap DataFusion's Parquet sink so we can surface live sink-side row counts
  // once sorted batches start flowing into the writer.
  inner: ParquetSink,
  metrics: ExecutionPlanMetricsSet,
  sink_rows: Count,
}

impl TrackingParquetSink {
  /// Build a sink and register its global row counter.
  fn new(conf: FileSinkConfig, parquet_options: TableParquetOptions) -> Self {
    let metrics = ExecutionPlanMetricsSet::new();
    let sink_rows = MetricBuilder::new(&metrics).global_counter(SINK_ROWS_METRIC);
    Self {
      config: conf.clone(),
      inner: ParquetSink::new(conf, parquet_options),
      metrics,
      sink_rows,
    }
  }

  /// Delete every object recorded by the inner sink after a failed write.
  async fn cleanup_written_files(&self, context: &Arc<TaskContext>) -> DataFusionResult<()> {
    let written_files = self.inner.written();
    if written_files.is_empty() {
      return Ok(());
    }

    let object_store = context
      .runtime_env()
      .object_store(&self.config.object_store_url)?;
    let mut cleanup_error = None;
    for path in written_files.keys() {
      if let Err(error) = object_store.delete(path).await {
        if cleanup_error.is_none() {
          cleanup_error = Some(DataFusionError::ObjectStore(Box::new(error)));
        }
      }
    }

    if let Some(error) = cleanup_error {
      Err(error)
    } else {
      Ok(())
    }
  }
}

impl DisplayAs for TrackingParquetSink {
  fn fmt_as(&self, t: DisplayFormatType, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    self.inner.fmt_as(t, f)
  }
}

#[async_trait]
impl DataSink for TrackingParquetSink {
  fn as_any(&self) -> &dyn Any {
    self
  }

  fn metrics(&self) -> Option<MetricsSet> {
    Some(self.metrics.clone_inner())
  }

  fn schema(&self) -> &SchemaRef {
    self.inner.schema()
  }

  async fn write_all(
    &self,
    data: SendableRecordBatchStream,
    context: &Arc<TaskContext>,
  ) -> DataFusionResult<u64> {
    let schema = Arc::clone(self.inner.schema());
    let sink_rows = self.sink_rows.clone();
    let tracked_stream = data.map(move |batch| {
      if let Ok(batch) = &batch {
        sink_rows.add(batch.num_rows());
      }
      batch
    });
    self
      .inner
      .write_all(
        Box::pin(RecordBatchStreamAdapter::new(schema, tracked_stream)),
        context,
      )
      .await
  }
}

#[derive(Clone, Debug)]
/// Executes every sorted input partition as a concurrent Parquet write task.
///
/// The node exposes one output partition containing the combined written-row count.
struct ConcurrentPartitionedParquetSinkExec {
  input: Arc<dyn ExecutionPlan>,
  sink: Arc<TrackingParquetSink>,
  count_schema: SchemaRef,
  sort_order: Option<LexRequirement>,
  cache: PlanProperties,
}

impl ConcurrentPartitionedParquetSinkExec {
  /// Build an eager cooperative sink node around a partitioned input plan.
  fn new(
    input: Arc<dyn ExecutionPlan>,
    sink: Arc<TrackingParquetSink>,
    sort_order: Option<LexRequirement>,
  ) -> Self {
    let count_schema = count_schema();
    let eq_properties = EquivalenceProperties::new(Arc::clone(&count_schema));
    let cache = PlanProperties::new(
      eq_properties,
      Partitioning::UnknownPartitioning(1),
      input.pipeline_behavior(),
      input.boundedness(),
    )
    .with_scheduling_type(SchedulingType::Cooperative)
    .with_evaluation_type(EvaluationType::Eager);
    Self {
      input,
      sink,
      count_schema,
      sort_order,
      cache,
    }
  }
}

impl DisplayAs for ConcurrentPartitionedParquetSinkExec {
  fn fmt_as(&self, t: DisplayFormatType, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match t {
      DisplayFormatType::Default | DisplayFormatType::Verbose => {
        write!(f, "ConcurrentPartitionedParquetSinkExec: sink=")?;
        self.sink.fmt_as(t, f)
      }
      DisplayFormatType::TreeRender => self.sink.fmt_as(t, f),
    }
  }
}

impl ExecutionPlan for ConcurrentPartitionedParquetSinkExec {
  fn name(&self) -> &'static str {
    "ConcurrentPartitionedParquetSinkExec"
  }

  fn as_any(&self) -> &dyn Any {
    self
  }

  fn properties(&self) -> &PlanProperties {
    &self.cache
  }

  fn required_input_distribution(&self) -> Vec<Distribution> {
    vec![Distribution::UnspecifiedDistribution]
  }

  fn required_input_ordering(
    &self,
  ) -> Vec<Option<datafusion::physical_expr::OrderingRequirements>> {
    vec![self.sort_order.as_ref().cloned().map(Into::into)]
  }

  fn maintains_input_order(&self) -> Vec<bool> {
    vec![true]
  }

  fn children(&self) -> Vec<&Arc<dyn ExecutionPlan>> {
    vec![&self.input]
  }

  fn with_new_children(
    self: Arc<Self>,
    children: Vec<Arc<dyn ExecutionPlan>>,
  ) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
    Ok(Arc::new(Self::new(
      Arc::clone(&children[0]),
      Arc::clone(&self.sink),
      self.sort_order.clone(),
    )))
  }

  fn execute(
    &self,
    partition: usize,
    context: Arc<TaskContext>,
  ) -> DataFusionResult<SendableRecordBatchStream> {
    if partition != 0 {
      return Err(DataFusionError::Execution(format!(
        "{} can only be called on partition 0",
        self.name()
      )));
    }

    let count_schema = Arc::clone(&self.count_schema);
    let input = Arc::clone(&self.input);
    let sink = Arc::clone(&self.sink);
    let stream = futures_util::stream::once(async move {
      run_concurrent_partitioned_parquet_writes(input, sink, &context)
        .await
        .map(make_count_batch)
    });
    Ok(Box::pin(RecordBatchStreamAdapter::new(
      count_schema,
      stream,
    )))
  }

  fn metrics(&self) -> Option<MetricsSet> {
    self.sink.metrics()
  }
}

/// Execute and join all input partitions, cleaning partial files when any task fails.
async fn run_concurrent_partitioned_parquet_writes(
  input: Arc<dyn ExecutionPlan>,
  sink: Arc<TrackingParquetSink>,
  context: &Arc<TaskContext>,
) -> DataFusionResult<u64> {
  let mut write_tasks = JoinSet::new();
  for partition in 0..input.output_partitioning().partition_count() {
    let input = Arc::clone(&input);
    let sink = Arc::clone(&sink);
    let context = Arc::clone(context);
    write_tasks.spawn(async move {
      let data = execute_input_stream(
        Arc::clone(&input),
        Arc::clone(sink.schema()),
        partition,
        Arc::clone(&context),
      )?;
      sink.write_all(data, &context).await
    });
  }

  let mut rows_written = 0;
  let mut first_error = None;
  while let Some(result) = write_tasks.join_next().await {
    match result {
      Ok(Ok(count)) => rows_written += count,
      Ok(Err(error)) => {
        if first_error.is_none() {
          first_error = Some(error);
        }
      }
      Err(error) if error.is_panic() => std::panic::resume_unwind(error.into_panic()),
      Err(error) => {
        if first_error.is_none() {
          first_error = Some(DataFusionError::Execution(format!(
            "partitioned parquet write task failed: {error}"
          )));
        }
      }
    }
  }

  if let Some(error) = first_error {
    if let Err(cleanup_error) = sink.cleanup_written_files(context).await {
      return Err(DataFusionError::Execution(format!(
        "partitioned parquet write failed: {error}; cleanup failed: {cleanup_error}"
      )));
    }
    return Err(error);
  }

  Ok(rows_written)
}

/// Build the one-column schema returned by custom sink execution plans.
fn count_schema() -> SchemaRef {
  Arc::new(Schema::new(vec![Field::new(
    "count",
    DataType::UInt64,
    false,
  )]))
}

/// Wrap a written-row count in the custom sink execution plan's output batch.
fn make_count_batch(count: u64) -> RecordBatch {
  RecordBatch::try_new(
    count_schema(),
    vec![Arc::new(UInt64Array::from(vec![count]))],
  )
  .expect("count batch should always be valid")
}

#[derive(Default, Clone, Copy)]
/// Aggregates row, spill, and compute metrics across a physical-plan tree.
struct PlanProgressMetrics {
  rows_read: u64,
  rows_written: u64,
  spill_count: u64,
  spilled_bytes: u64,
  elapsed_compute_nanos: u64,
}

/// Execute an aggregate DataFrame while polling physical-plan metrics for progress.
async fn collect_df_with_metric_polling(
  df: engine::DataFrame,
  progress_bar: &ProgressBar,
  total_input_rows: u64,
  base_message: &str,
  explain: bool,
) -> Result<Vec<RecordBatch>> {
  let (state, logical_plan) = df.into_parts();
  explain_dataframe_verbose(explain, base_message, &state, &logical_plan, true).await?;
  let physical_plan = state.create_physical_plan(&logical_plan).await?;
  explain_physical_plan(explain, base_message, &physical_plan);
  let task_ctx = Arc::new(TaskContext::from(&state));
  let stage_start = Instant::now();

  let done = Arc::new(AtomicBool::new(false));
  let poller = if progress_bar.is_hidden() {
    None
  } else {
    let plan = Arc::clone(&physical_plan);
    let bar = progress_bar.clone();
    let done = Arc::clone(&done);
    let base_message = base_message.to_string();
    Some(std::thread::spawn(move || {
      while !done.load(Ordering::Relaxed) {
        update_metric_count_bar(
          &bar,
          &base_message,
          "finalizing aggregates",
          collect_plan_progress(plan.as_ref()),
          total_input_rows,
        );
        std::thread::sleep(Duration::from_millis(500));
      }
    }))
  };

  let result = collect(Arc::clone(&physical_plan), task_ctx).await;
  done.store(true, Ordering::Relaxed);
  if let Some(poller) = poller {
    let _ = poller.join();
  }

  explain_stage_completion(
    explain,
    base_message,
    stage_start.elapsed(),
    &physical_plan,
    collect_plan_progress(physical_plan.as_ref()),
  );
  result.map_err(Into::into)
}

/// Build and execute the single-file or concurrent partitioned Parquet sink plan.
async fn write_parquet_with_metric_polling(
  df: engine::DataFrame,
  write_path: &str,
  partition_by: Vec<String>,
  partitioned_write: Option<PartitionedWriteConfig>,
  writer_options: TableParquetOptions,
  progress_bar: &ProgressBar,
  total_input_rows: u64,
  explain: bool,
) -> Result<u64> {
  let (state, logical_plan) = df.into_parts();
  explain_dataframe_verbose(
    explain,
    "Writing parquet output input query",
    &state,
    &logical_plan,
    false,
  )
  .await?;
  let task_ctx = Arc::new(TaskContext::from(&state));
  let is_partitioned_write = partitioned_write.is_some();
  let physical_plan: Arc<dyn ExecutionPlan> =
    if let Some(partitioned_write) = partitioned_write.as_ref() {
      let input_plan = state.create_physical_plan(&logical_plan).await?;
      let rewritten_input = preserve_partitioned_sort_execs(input_plan, Some(partitioned_write))
        .map_err(anyhow::Error::from)?;
      let parsed_url = ListingTableUrl::parse(write_path)?;
      let sink_config = FileSinkConfig {
        original_url: write_path.to_string(),
        object_store_url: parsed_url.object_store(),
        file_group: Default::default(),
        table_paths: vec![parsed_url],
        output_schema: rewritten_input.schema(),
        table_partition_cols: partition_by
          .iter()
          .map(|column| (column.to_string(), DataType::Null))
          .collect(),
        insert_op: InsertOp::Append,
        keep_partition_by_columns: state.config_options().execution.keep_partition_by_columns,
        file_extension: "parquet".to_string(),
      };
      let sink = Arc::new(TrackingParquetSink::new(sink_config, writer_options));
      Arc::new(ConcurrentPartitionedParquetSinkExec::new(
        rewritten_input,
        sink,
        None,
      ))
    } else {
      let format = Arc::new(TrackingParquetFormatFactory::new(writer_options));
      let file_type = format_as_file_type(format);
      let copy_plan = LogicalPlanBuilder::copy_to(
        logical_plan,
        write_path.to_string(),
        file_type,
        Default::default(),
        partition_by,
      )?
      .build()?;
      state.create_physical_plan(&copy_plan).await?
    };
  explain_physical_plan(explain, "Writing parquet output", &physical_plan);
  let stage_start = Instant::now();

  let done = Arc::new(AtomicBool::new(false));
  let poller = if progress_bar.is_hidden() {
    None
  } else {
    let plan = Arc::clone(&physical_plan);
    let bar = progress_bar.clone();
    let done = Arc::clone(&done);
    Some(std::thread::spawn(move || {
      let mut current_phase = None;
      while !done.load(Ordering::Relaxed) {
        update_write_stage_bar(
          &bar,
          collect_plan_progress(plan.as_ref()),
          total_input_rows,
          is_partitioned_write,
          &mut current_phase,
        );
        std::thread::sleep(Duration::from_millis(500));
      }
    }))
  };

  let result = collect(Arc::clone(&physical_plan), task_ctx).await;
  done.store(true, Ordering::Relaxed);
  if let Some(poller) = poller {
    let _ = poller.join();
  }

  explain_stage_completion(
    explain,
    "Writing parquet output",
    stage_start.elapsed(),
    &physical_plan,
    collect_plan_progress(physical_plan.as_ref()),
  );
  extract_written_row_count(&result?)
}

#[allow(dead_code)]
/// Write one ordered stream into sequential files using direct Arrow writers.
///
/// This retained fallback preserves global input order but cannot write files concurrently.
/// The active multi-file path instead uses partition-local sorts and concurrent DataFusion sinks.
async fn write_ordered_multi_file_parquet_with_metric_polling(
  df: engine::DataFrame,
  output_plan: &OutputPlan,
  compression: parquet::basic::Compression,
  kv_metadata: &[KeyValue],
  progress_bar: &ProgressBar,
  total_input_rows: u64,
) -> Result<u64> {
  let output_schema = Arc::new(df.schema().as_arrow().clone());
  let output_paths = output_paths(output_plan)?;
  let target_rows = target_rows_per_file(output_plan.parts, total_input_rows);
  let (state, logical_plan) = df.into_parts();
  let physical_plan = state.create_physical_plan(&logical_plan).await?;
  let task_ctx = Arc::new(TaskContext::from(&state));
  let mut writers = output_paths
    .iter()
    .map(|path| create_output_writer(path, &output_schema, compression))
    .collect::<Result<Vec<_>>>()?;
  let written_rows = Arc::new(std::sync::atomic::AtomicU64::new(0));

  let done = Arc::new(AtomicBool::new(false));
  let poller = if progress_bar.is_hidden() {
    None
  } else {
    let plan = Arc::clone(&physical_plan);
    let bar = progress_bar.clone();
    let done = Arc::clone(&done);
    let written_rows = Arc::clone(&written_rows);
    Some(std::thread::spawn(move || {
      let mut current_phase = None;
      while !done.load(Ordering::Relaxed) {
        let mut metrics = collect_plan_progress(plan.as_ref());
        metrics.rows_written = written_rows.load(Ordering::Relaxed);
        update_write_stage_bar(&bar, metrics, total_input_rows, true, &mut current_phase);
        std::thread::sleep(Duration::from_millis(500));
      }
    }))
  };

  let write_result = async {
    let mut stream = execute_stream(Arc::clone(&physical_plan), Arc::clone(&task_ctx))?;
    let mut writer_idx = 0usize;

    while let Some(batch) = stream.next().await {
      let batch = batch?;
      let writer_count = writers.len();
      let mut batch_offset = 0usize;
      while batch_offset < batch.num_rows() {
        let rows_remaining = batch.num_rows() - batch_offset;
        let writer = writers
          .get_mut(writer_idx)
          .context("missing parquet writer for output file")?;
        let rows_for_writer = if target_rows > 0 && writer_idx + 1 < writer_count {
          let writer_remaining = target_rows.saturating_sub(writer.rows_written).max(1) as usize;
          writer_remaining.min(rows_remaining)
        } else {
          rows_remaining
        };
        let batch_slice = batch.slice(batch_offset, rows_for_writer);
        write_batches(writer, &batch_slice)?;
        written_rows.fetch_add(rows_for_writer as u64, Ordering::Relaxed);
        batch_offset += rows_for_writer;
        if writer_idx + 1 < writer_count && target_rows > 0 && writer.rows_written >= target_rows {
          writer_idx += 1;
        }
      }
    }

    finalize_writers(writers, kv_metadata)?;
    Ok(written_rows.load(Ordering::Relaxed))
  }
  .await;

  done.store(true, Ordering::Relaxed);
  if let Some(poller) = poller {
    let _ = poller.join();
  }

  if write_result.is_err() {
    cleanup_output_paths(&output_paths)?;
  }

  write_result
}

#[allow(dead_code)]
/// Preserve custom range-partition sorting when partitioned output is configured.
fn preserve_partitioned_sort_execs(
  plan: Arc<dyn ExecutionPlan>,
  partitioned_write: Option<&PartitionedWriteConfig>,
) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
  let Some(partitioned_write) = partitioned_write else {
    return Ok(plan);
  };
  insert_partitioned_sort_exec(plan, partitioned_write)
}

/// Recursively insert the repartition-and-sort node where both control columns exist.
fn insert_partitioned_sort_exec(
  plan: Arc<dyn ExecutionPlan>,
  partitioned_write: &PartitionedWriteConfig,
) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
  let schema = plan.schema();
  let partition_column_index = schema.index_of(&partitioned_write.partition_column);
  let sort_column_index = schema.index_of(&partitioned_write.sort_column);
  if let (Ok(partition_column_index), Ok(sort_column_index)) =
    (partition_column_index, sort_column_index)
  {
    return build_partitioned_sort_exec(
      plan,
      partitioned_write,
      partition_column_index,
      sort_column_index,
    );
  }

  let children = plan.children();
  if children.is_empty() {
    return Err(DataFusionError::Execution(format!(
      "unable to insert partitioned sort: columns '{}' and '{}' were not both available in plan schema {:?}",
      partitioned_write.partition_column,
      partitioned_write.sort_column,
      schema
        .fields()
        .iter()
        .map(|field| field.name().clone())
        .collect::<Vec<_>>(),
    )));
  }

  let rewritten_children = children
    .into_iter()
    .map(|child| insert_partitioned_sort_exec(Arc::clone(child), partitioned_write))
    .collect::<DataFusionResult<Vec<_>>>()?;
  plan.with_new_children(rewritten_children)
}

/// Hash by output range and sort each resulting partition by spatial code.
fn build_partitioned_sort_exec(
  plan: Arc<dyn ExecutionPlan>,
  partitioned_write: &PartitionedWriteConfig,
  partition_column_index: usize,
  sort_column_index: usize,
) -> DataFusionResult<Arc<dyn ExecutionPlan>> {
  let repartitioned_input: Arc<dyn ExecutionPlan> = Arc::new(RepartitionExec::try_new(
    plan,
    Partitioning::Hash(
      vec![Arc::new(PhysicalColumn::new(
        &partitioned_write.partition_column,
        partition_column_index,
      ))],
      multi_file_sort_partition_count(partitioned_write.bucket_count),
    ),
  )?);
  let sort_order: LexOrdering = [PhysicalSortExpr {
    expr: Arc::new(PhysicalColumn::new(
      &partitioned_write.sort_column,
      sort_column_index,
    )),
    options: SortOptions {
      descending: false,
      nulls_first: false,
    },
  }]
  .into();
  let sorted_input: Arc<dyn ExecutionPlan> =
    Arc::new(SortExec::new(sort_order, repartitioned_input).with_preserve_partitioning(true));
  if partitioned_write.drop_sort_column_after_sort {
    let sorted_schema = sorted_input.schema();
    let projection_exprs = sorted_schema
      .fields()
      .iter()
      .enumerate()
      .filter(|(_, field)| field.name() != &partitioned_write.sort_column)
      .map(|(index, field)| ProjectionExpr {
        expr: Arc::new(PhysicalColumn::new(field.name(), index)),
        alias: field.name().to_string(),
      })
      .collect::<Vec<_>>();
    Ok(Arc::new(ProjectionExec::try_new(
      projection_exprs,
      sorted_input,
    )?))
  } else {
    Ok(sorted_input)
  }
}

/// Collect progress metrics from a complete physical-plan tree.
fn collect_plan_progress(plan: &dyn ExecutionPlan) -> PlanProgressMetrics {
  let mut metrics = PlanProgressMetrics::default();
  accumulate_plan_progress(plan, &mut metrics);
  metrics
}

/// Recursively aggregate progress metrics without double-counting parent output rows.
fn accumulate_plan_progress(plan: &dyn ExecutionPlan, metrics: &mut PlanProgressMetrics) -> bool {
  let mut child_has_row_metric = false;
  for child in plan.children() {
    child_has_row_metric |= accumulate_plan_progress(child.as_ref(), metrics);
  }

  let mut plan_has_row_metric = false;
  if let Some(plan_metrics) = plan.metrics() {
    metrics.spill_count += plan_metrics.spill_count().unwrap_or(0) as u64;
    metrics.spilled_bytes += plan_metrics.spilled_bytes().unwrap_or(0) as u64;
    metrics.elapsed_compute_nanos += plan_metrics.elapsed_compute().unwrap_or(0) as u64;
    if let Some(rows_written) = plan_metrics.sum_by_name(SINK_ROWS_METRIC) {
      metrics.rows_written += rows_written.as_usize() as u64;
    }
    if let Some(output_rows) = plan_metrics.output_rows() {
      plan_has_row_metric = true;
      if !child_has_row_metric {
        metrics.rows_read += output_rows as u64;
      }
    }
  }
  child_has_row_metric || plan_has_row_metric
}

#[derive(Clone, Copy, PartialEq, Eq)]
/// Identifies the observable phase of the final physical write plan.
enum WriteStagePhase {
  Reading,
  Sorting,
  Writing,
}

/// Update aggregate-stage progress from physical-plan row and spill metrics.
fn update_metric_count_bar(
  progress_bar: &ProgressBar,
  base_message: &str,
  post_read_message: &str,
  metrics: PlanProgressMetrics,
  total_input_rows: u64,
) {
  if progress_bar.is_hidden() {
    return;
  }
  if total_input_rows > 0 {
    progress_bar.set_position(metrics.rows_read.min(total_input_rows));
  }
  progress_bar.set_message(format_metric_progress_message(
    base_message,
    post_read_message,
    metrics,
    total_input_rows,
    false,
  ));
}

/// Derive and render the read, sort, or write phase of the final sink plan.
fn update_write_stage_bar(
  progress_bar: &ProgressBar,
  metrics: PlanProgressMetrics,
  total_input_rows: u64,
  partitioned_write: bool,
  current_phase: &mut Option<WriteStagePhase>,
) {
  if progress_bar.is_hidden() {
    return;
  }

  let phase = if metrics.rows_written > 0 {
    WriteStagePhase::Writing
  } else if total_input_rows > 0 && metrics.rows_read >= total_input_rows {
    WriteStagePhase::Sorting
  } else {
    WriteStagePhase::Reading
  };

  if current_phase != &Some(phase) {
    match phase {
      WriteStagePhase::Reading | WriteStagePhase::Writing => {
        progress_bar.set_style(count_bar_style("rows"));
        progress_bar.set_length(total_input_rows.max(1));
      }
      WriteStagePhase::Sorting => {
        progress_bar.set_style(message_only_style());
      }
    }
    *current_phase = Some(phase);
  }

  match phase {
    WriteStagePhase::Reading => {
      progress_bar.set_message(write_stage_message(phase, partitioned_write).to_string());
      progress_bar.set_position(metrics.rows_read.min(total_input_rows));
    }
    WriteStagePhase::Sorting => {
      progress_bar.set_message(write_stage_message(phase, partitioned_write).to_string());
    }
    WriteStagePhase::Writing => {
      progress_bar.set_message(write_stage_message(phase, partitioned_write).to_string());
      progress_bar.set_position(metrics.rows_written.min(total_input_rows));
    }
  }
}

/// Return the user-facing progress label for one output phase and layout.
fn write_stage_message(phase: WriteStagePhase, partitioned_write: bool) -> &'static str {
  match (phase, partitioned_write) {
    (WriteStagePhase::Reading, false) => "Preparing output — reading rows into sort buffers",
    (WriteStagePhase::Sorting, false) => "Preparing output — sorting buffered rows",
    (WriteStagePhase::Writing, false) => "Writing parquet file",
    (WriteStagePhase::Reading, true) => {
      "Preparing output — reading rows into sort buffers for multiple files"
    }
    (WriteStagePhase::Sorting, true) => "Preparing output — sorting rows for multiple files",
    (WriteStagePhase::Writing, true) => "Writing parquet files",
  }
}

/// Format live row, spill, and post-read activity for a progress bar.
fn format_metric_progress_message(
  base_message: &str,
  post_read_message: &str,
  metrics: PlanProgressMetrics,
  total_input_rows: u64,
  allow_sink_rows: bool,
) -> String {
  if allow_sink_rows && metrics.rows_written > 0 {
    let rows_written = if total_input_rows > 0 {
      metrics.rows_written.min(total_input_rows)
    } else {
      metrics.rows_written
    };
    return if total_input_rows > 0 {
      format!(
        "{base_message} — writing {rows_written}/{total_input_rows} rows ({}%)",
        format_progress_percent(rows_written, total_input_rows)
      )
    } else {
      format!("{base_message} — writing {rows_written} rows")
    };
  }

  let mut message = if metrics.rows_read == 0 {
    format!("{base_message} — scanning source")
  } else if total_input_rows > 0 && metrics.rows_read < total_input_rows {
    base_message.to_string()
  } else if total_input_rows > 0 {
    format!(
      "{base_message} — {}",
      format_post_read_activity(post_read_message, metrics)
    )
  } else if metrics.rows_read > 0 {
    base_message.to_string()
  } else {
    base_message.to_string()
  };

  if total_input_rows == 0 || metrics.rows_read < total_input_rows {
    if metrics.spill_count > 0 || metrics.spilled_bytes > 0 {
      message.push_str(&format!(
        " — spills {} ({})",
        metrics.spill_count,
        format_bytes(metrics.spilled_bytes)
      ));
    }
  }
  message
}

/// Format aggregate or sort activity after all source rows have entered the plan.
fn format_post_read_activity(post_read_message: &str, metrics: PlanProgressMetrics) -> String {
  let mut message = if metrics.spill_count > 0 || metrics.spilled_bytes > 0 {
    format!(
      "{post_read_message} — spills {} ({})",
      metrics.spill_count,
      format_bytes(metrics.spilled_bytes)
    )
  } else {
    format!("{post_read_message} in memory")
  };

  if metrics.elapsed_compute_nanos > 0 {
    message.push_str(&format!(
      " — compute {}",
      format_elapsed(Duration::from_nanos(metrics.elapsed_compute_nanos))
    ));
  }
  message
}

/// Calculate an integer progress percentage without overflowing intermediate multiplication.
fn format_progress_percent(rows: u64, total_rows: u64) -> u64 {
  if total_rows == 0 {
    return 0;
  }
  (((rows as u128) * 100) / total_rows as u128) as u64
}

/// Extract a dataset extent from one-row aggregate output.
fn extract_extent_from_aggregate_batches(
  batches: &[RecordBatch],
) -> Result<crate::analysis::Extent2D> {
  let Some(batch) = batches.first() else {
    bail!("unable to determine dataset full extent");
  };
  if batch.num_rows() == 0 {
    bail!("unable to determine dataset full extent");
  }
  let xmin = extract_float_aggregate_value(batch, 0, "xmin")?;
  let ymin = extract_float_aggregate_value(batch, 1, "ymin")?;
  let xmax = extract_float_aggregate_value(batch, 2, "xmax")?;
  let ymax = extract_float_aggregate_value(batch, 3, "ymax")?;
  Ok(crate::analysis::Extent2D {
    xmin,
    ymin,
    xmax,
    ymax,
  })
}

/// Extract one non-null f64 value from a one-row aggregate batch.
fn extract_float_aggregate_value(
  batch: &RecordBatch,
  column_index: usize,
  label: &str,
) -> Result<f64> {
  let values = batch
    .column(column_index)
    .as_any()
    .downcast_ref::<Float64Array>()
    .with_context(|| format!("analysis aggregate column '{label}' was not Float64"))?;
  if values.is_null(0) {
    bail!("unable to determine dataset full extent");
  }
  Ok(values.value(0))
}

/// Convert sink output batches into the single total row count.
fn extract_written_row_count(batches: &[RecordBatch]) -> Result<u64> {
  let Some(batch) = batches.first() else {
    return Ok(0);
  };
  if batch.num_rows() == 0 {
    return Ok(0);
  }
  let values = batch
    .column(0)
    .as_any()
    .downcast_ref::<UInt64Array>()
    .context("write result count column was not UInt64")?;
  Ok(values.value(0))
}

#[allow(dead_code)]
/// Remove direct-writer output files after a failed sequential write.
///
/// Missing files do not count as cleanup failures because another failure path may have
/// removed them already.
fn cleanup_output_paths(paths: &[PathBuf]) -> Result<()> {
  let mut cleanup_error = None;
  for path in paths {
    if let Err(error) = std::fs::remove_file(path)
      && error.kind() != std::io::ErrorKind::NotFound
      && cleanup_error.is_none()
    {
      cleanup_error = Some(error);
    }
  }
  if let Some(error) = cleanup_error {
    return Err(error).context("cleanup output files");
  }
  Ok(())
}

/// Resolve an explicit geometry column or require provider inference.
fn resolve_input_geometry_spec(
  schema: &arrow_schema::Schema,
  inferred_geometry_spec: Option<GeometrySpec>,
  explicit_geometry_column: Option<&str>,
) -> Result<GeometrySpec> {
  if let Some(column) = explicit_geometry_column {
    schema
      .field_with_name(column)
      .with_context(|| format!("missing geometry column '{column}'"))?;
    return Ok(GeometrySpec {
      column: column.to_string(),
      encoding: GeometryEncoding::Wkb,
      geometry_kind: None,
    });
  }

  inferred_geometry_spec.context("unable to resolve geometry spec")
}

/// Reject covering configurations that would conflict with pass-through or source columns.
fn validate_covering_configuration(
  covering: bool,
  no_optimization: bool,
  schema: &arrow_schema::Schema,
) -> Result<()> {
  if !covering {
    return Ok(());
  }
  if no_optimization {
    bail!("--covering cannot be used with --no-optimization");
  }
  if schema.field_with_name(COVERING_BBOX_COLUMN).is_ok() {
    bail!("--covering would overwrite existing input column '{COVERING_BBOX_COLUMN}'");
  }
  Ok(())
}

/// Reject geometry categories and dimensions not implemented by display encoding.
fn ensure_supported(analysis: &DisplayJobAnalysis) -> Result<()> {
  if analysis.has_z || analysis.has_m {
    bail!("display optimization does not yet support Z/M geometries")
  }
  if matches!(analysis.geometry_type, DisplayGeometryType::MultiPoint) {
    bail!("display optimization does not yet support multipoint geometries")
  }
  Ok(())
}

/// Build GeoParquet and geodisplay key-value metadata for the final file set.
fn build_output_metadata(
  source_metadata: &SourceDatasetMetadata,
  analysis: &DisplayJobAnalysis,
  encodings: &[crate::multiscale::GeometryEncoding],
  covering: bool,
) -> Result<Vec<KeyValue>> {
  let mut metadata = source_metadata.passthrough_kv.clone();
  metadata.push(KeyValue {
    key: "geo".to_string(),
    value: Some(build_geo_metadata(source_metadata, analysis, covering)?),
  });

  let geodisplay = match analysis.geometry_family {
    GeometryFamily::Point => GeodisplayMetadata::point(DisplayIndexZ::new(
      POINT_Z_CODE_COLUMN,
      POINT_X_COLUMN,
      POINT_Y_COLUMN,
      DEFAULT_COORDINATE_PRECISION,
      analysis.full_extent,
      analysis.spatial_reference.wkid,
      None,
      false,
      false,
    )),
    GeometryFamily::NonPoint => GeodisplayMetadata::xz_with_parent(
      DISPLAY_COLUMN,
      DisplayIndexXz::new(
        XZ_CODE_COLUMN,
        "esriPBF",
        analysis.geometry_type.as_str(),
        BOUNDS_COLUMN,
        analysis.full_extent,
        DEFAULT_MAX_LEVEL,
        analysis.spatial_reference.wkid,
        None,
        false,
        false,
        metadata_levels(encodings),
      ),
    ),
  };
  metadata.push(KeyValue {
    key: "geodisplay".to_string(),
    value: Some(serde_json::to_string(&geodisplay)?),
  });
  Ok(metadata)
}

/// Serialize GeoParquet 1.1 metadata for the optimized geometry column.
fn build_geo_metadata(
  source_metadata: &SourceDatasetMetadata,
  analysis: &DisplayJobAnalysis,
  covering: bool,
) -> Result<String> {
  let geometry_metadata = source_metadata
    .geometry
    .as_ref()
    .filter(|geometry| geometry.column == analysis.geometry_spec.column);
  let has_z = geometry_metadata.is_some_and(|geometry| geometry.has_z);
  let has_m = geometry_metadata.is_some_and(|geometry| geometry.has_m);
  let geometry_types = geometry_metadata
    .filter(|geometry| !geometry.geometry_types.is_empty())
    .map(|geometry| geometry.geometry_types.clone())
    .unwrap_or_else(|| vec![fallback_geometry_kind(analysis.geometry_type)]);

  let mut column = serde_json::Map::new();
  column.insert("encoding".to_string(), Value::String("WKB".to_string()));
  column.insert(
    "geometry_types".to_string(),
    Value::Array(
      geometry_types
        .into_iter()
        .map(|geometry_kind| {
          Ok(Value::String(geoparquet_geometry_type_name(
            geometry_kind,
            has_z,
            has_m,
          )?))
        })
        .collect::<Result<Vec<_>>>()?,
    ),
  );

  column.insert(
    "bbox".to_string(),
    serde_json::json!([
      analysis.full_extent.xmin,
      analysis.full_extent.ymin,
      analysis.full_extent.xmax,
      analysis.full_extent.ymax
    ]),
  );
  column.insert(
    "crs".to_string(),
    analysis
      .spatial_reference
      .projjson
      .clone()
      .unwrap_or_else(|| Value::Object(Default::default())),
  );
  if covering {
    column.insert("covering".to_string(), geo_covering_bbox_metadata());
  }

  let mut columns = serde_json::Map::new();
  columns.insert(analysis.geometry_spec.column.clone(), Value::Object(column));

  Ok(serde_json::to_string(&serde_json::json!({
      "version": "1.1.0",
      "primary_column": analysis.geometry_spec.column,
      "columns": columns,
  }))?)
}

/// Build GeoParquet 1.1 covering metadata that points into the generated bbox struct.
fn geo_covering_bbox_metadata() -> Value {
  serde_json::json!({
    "bbox": {
      "xmin": [COVERING_BBOX_COLUMN, "xmin"],
      "ymin": [COVERING_BBOX_COLUMN, "ymin"],
      "xmax": [COVERING_BBOX_COLUMN, "xmax"],
      "ymax": [COVERING_BBOX_COLUMN, "ymax"],
    }
  })
}

/// Map a display category back to a valid GeoParquet geometry kind when source types are absent.
fn fallback_geometry_kind(geometry_type: DisplayGeometryType) -> GeometryKind {
  match geometry_type {
    DisplayGeometryType::Point => GeometryKind::Point,
    DisplayGeometryType::MultiPoint => GeometryKind::MultiPoint,
    DisplayGeometryType::Polyline => GeometryKind::LineString,
    DisplayGeometryType::Polygon => GeometryKind::Polygon,
  }
}

/// Format a GeoParquet geometry type name with its declared Z/M dimensional suffix.
fn geoparquet_geometry_type_name(
  geometry_kind: GeometryKind,
  has_z: bool,
  has_m: bool,
) -> Result<String> {
  let base = match geometry_kind {
    GeometryKind::Point => "Point",
    GeometryKind::LineString => "LineString",
    GeometryKind::MultiPoint => "MultiPoint",
    GeometryKind::MultiLineString => "MultiLineString",
    GeometryKind::Polygon => "Polygon",
    GeometryKind::MultiPolygon => "MultiPolygon",
    GeometryKind::GeometryCollection => "GeometryCollection",
    GeometryKind::Unknown => return Err(anyhow::anyhow!("unsupported geometry kind metadata")),
  };
  let suffix = match (has_z, has_m) {
    (false, false) => "",
    (true, false) => " Z",
    (false, true) => " M",
    (true, true) => " ZM",
  };
  Ok(format!("{base}{suffix}"))
}

/// Build a row-count progress bar with the standard row unit.
fn row_bar(enabled: bool, message: &str, total_rows: u64) -> ProgressBar {
  count_bar(enabled, message, total_rows, "rows")
}

/// Build a standalone count progress bar for an arbitrary unit.
fn count_bar(enabled: bool, message: &str, total: u64, unit: &str) -> ProgressBar {
  if !enabled || !std::io::stderr().is_terminal() {
    return ProgressBar::hidden();
  }
  count_bar_with_parent(None, message, total, unit)
}

/// Build a count progress bar and register it beneath a shared multi-progress owner.
fn count_bar_with_parent(
  parent: Option<&MultiProgress>,
  message: &str,
  total: u64,
  unit: &str,
) -> ProgressBar {
  let bar = if let Some(parent) = parent {
    parent.add(ProgressBar::new(total.max(1)))
  } else {
    ProgressBar::new(total.max(1))
  };
  bar.set_style(count_bar_style(unit));
  bar.set_message(message.to_string());
  bar
}

/// Build the standard progress style for count-based stages.
fn count_bar_style(unit: &str) -> ProgressStyle {
  ProgressStyle::with_template(&format!(
    "{{msg:20}} [{{bar:40.cyan/blue}}] {{pos}}/{{len}} {unit} ({{percent}}%)"
  ))
  .expect("valid progress template")
  .progress_chars("=>-")
}

/// Build the spinner-free style used while row progress cannot advance during sorting.
fn message_only_style() -> ProgressStyle {
  ProgressStyle::with_template("{msg}").expect("valid message template")
}

/// Finish a spinner or message-only progress indicator with a terminal message.
fn finish_spinner(bar: &ProgressBar, message: String) {
  if bar.is_hidden() {
    return;
  }
  bar.set_style(message_only_style());
  bar.finish_with_message(format!("{message} in {}", format_elapsed(bar.elapsed())));
}

/// Complete a row progress bar at the known input count.
fn finish_row_bar(bar: &ProgressBar, total_rows: u64, message: String) {
  finish_count_bar(bar, total_rows, message);
}

/// Complete a generic count progress bar at its known total.
fn finish_count_bar(bar: &ProgressBar, total: u64, message: String) {
  if bar.is_hidden() {
    return;
  }
  bar.set_position(total.max(1));
  bar.set_style(message_only_style());
  bar.finish_with_message(format!("{message} in {}", format_elapsed(bar.elapsed())));
}

/// Print the resolved source, row-range, output, and execution configuration in explain mode.
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
    "[explain] output_path={} overwrite={} progress={}",
    options.output.display(),
    options.overwrite,
    options.progress,
  );
}

/// Print a stage-level planning note only when explain mode is enabled.
fn explain_stage_note(explain: bool, stage: &str, note: &str) {
  if !explain {
    return;
  }
  eprintln!("[explain] {stage}: {note}");
}

/// Configure DataFusion explain verbosity and disable optimizer shortcuts that hide diagnostics.
fn configure_explain_session(ctx: &engine::SessionContext, explain: bool) {
  if !explain {
    return;
  }
  let state_ref = ctx.state_ref();
  let mut state = state_ref.write();
  let explain_options = &mut state.config_mut().options_mut().explain;
  explain_options.logical_plan_only = false;
  explain_options.physical_plan_only = false;
  explain_options.show_statistics = true;
  explain_options.show_schema = false;
  explain_options.show_sizes = true;
  explain_options.format = ExplainFormat::Indent;
  explain_options.analyze_level = ExplainAnalyzeLevel::Dev;
}

/// Print a logical plan and optionally execute DataFusion's verbose analyzed explain plan.
///
/// Aggregate stages request analyzed output to expose runtime metrics. Write input plans avoid
/// analyzed execution because that would perform the expensive source and transform work twice.
async fn explain_dataframe_verbose(
  explain: bool,
  stage: &str,
  state: &SessionState,
  logical_plan: &LogicalPlan,
  run_analyze_verbose: bool,
) -> Result<()> {
  if !explain {
    return Ok(());
  }

  let explain_verbose = engine::DataFrame::new(state.clone(), logical_plan.clone())
    .explain_with_options(
      ExplainOption::default()
        .with_verbose(true)
        .with_analyze(false)
        .with_format(ExplainFormat::Indent),
    )?
    .to_string()
    .await?;
  eprintln!("[explain] {stage} DataFusion EXPLAIN VERBOSE:");
  eprintln!("{explain_verbose}");

  if run_analyze_verbose {
    let explain_analyze_verbose = engine::DataFrame::new(state.clone(), logical_plan.clone())
      .explain_with_options(
        ExplainOption::default()
          .with_verbose(true)
          .with_analyze(true)
          .with_format(ExplainFormat::Indent),
      )?
      .to_string()
      .await?;
    eprintln!("[explain] {stage} DataFusion EXPLAIN ANALYZE VERBOSE:");
    eprintln!("{explain_analyze_verbose}");
  } else {
    eprintln!(
      "[explain] {stage}: skipping DataFusion EXPLAIN ANALYZE VERBOSE re-execution because this stage has write side effects; see the post-run physical-plan metrics below"
    );
  }

  Ok(())
}

/// Print one physical plan before execution when explain mode is enabled.
fn explain_physical_plan(explain: bool, stage: &str, plan: &Arc<dyn ExecutionPlan>) {
  if !explain {
    return;
  }
  eprintln!(
    "[explain] {stage} physical plan (output_partitions={}):",
    plan.output_partitioning().partition_count()
  );
  eprintln!(
    "{}",
    displayable(plan.as_ref())
      .set_show_statistics(true)
      .indent(true)
  );
}

/// Print elapsed time, final metrics, and operator hotspots for a completed stage.
fn explain_stage_completion(
  explain: bool,
  stage: &str,
  elapsed: Duration,
  plan: &Arc<dyn ExecutionPlan>,
  metrics: PlanProgressMetrics,
) {
  if !explain {
    return;
  }
  eprintln!("[timing] {stage}: {}", format_elapsed_debug(elapsed));
  eprintln!(
    "[metrics] {stage}: rows_read={} rows_written={} spill_count={} spilled={} compute={}",
    metrics.rows_read,
    metrics.rows_written,
    metrics.spill_count,
    format_bytes(metrics.spilled_bytes),
    format_elapsed_debug(Duration::from_nanos(metrics.elapsed_compute_nanos)),
  );
  explain_physical_plan_with_metrics(explain, stage, plan);
  explain_operator_hotspots(explain, stage, plan);
}

/// Print a physical plan annotated with its complete DataFusion metric set.
fn explain_physical_plan_with_metrics(explain: bool, stage: &str, plan: &Arc<dyn ExecutionPlan>) {
  if !explain {
    return;
  }

  let metric_types = vec![MetricType::SUMMARY, MetricType::DEV];
  eprintln!("[explain] {stage} physical plan with metrics (DataFusion EXPLAIN ANALYZE):");
  eprintln!(
    "{}",
    datafusion::physical_plan::display::DisplayableExecutionPlan::with_metrics(plan.as_ref())
      .set_show_statistics(true)
      .set_metric_types(metric_types.clone())
      .indent(true)
  );
  eprintln!(
    "[explain] {stage} physical plan with full metrics (DataFusion EXPLAIN ANALYZE VERBOSE):"
  );
  eprintln!(
    "{}",
    datafusion::physical_plan::display::DisplayableExecutionPlan::with_full_metrics(plan.as_ref())
      .set_show_statistics(true)
      .set_metric_types(metric_types)
      .indent(true)
  );
}

#[derive(Clone, Debug)]
/// Captures one physical operator's metrics for explain-mode hotspot ranking.
struct OperatorMetricSnapshot {
  operator: String,
  depth: usize,
  output_partitions: usize,
  output_rows: u64,
  rows_written: u64,
  spill_count: u64,
  spilled_bytes: u64,
  elapsed_compute_nanos: u64,
}

/// Rank physical operators by measured compute, spill, and row metrics.
fn explain_operator_hotspots(explain: bool, stage: &str, plan: &Arc<dyn ExecutionPlan>) {
  if !explain {
    return;
  }

  let mut snapshots = Vec::new();
  collect_operator_metric_snapshots(plan.as_ref(), 0, &mut snapshots);
  snapshots.retain(|snapshot| {
    snapshot.elapsed_compute_nanos > 0
      || snapshot.spill_count > 0
      || snapshot.spilled_bytes > 0
      || snapshot.rows_written > 0
      || snapshot.output_rows > 0
  });
  if snapshots.is_empty() {
    return;
  }

  snapshots.sort_by(|left, right| {
    right
      .elapsed_compute_nanos
      .cmp(&left.elapsed_compute_nanos)
      .then(right.spilled_bytes.cmp(&left.spilled_bytes))
      .then(right.spill_count.cmp(&left.spill_count))
      .then(right.rows_written.cmp(&left.rows_written))
      .then(right.output_rows.cmp(&left.output_rows))
      .then(left.depth.cmp(&right.depth))
  });

  eprintln!("[operator-metrics] {stage} hottest operators by compute:");
  for (index, snapshot) in snapshots.into_iter().take(12).enumerate() {
    eprintln!(
      "  {:>2}. operator={} depth={} out_partitions={} output_rows={} sink_rows={} compute={} spill_count={} spilled={}",
      index + 1,
      snapshot.operator,
      snapshot.depth,
      snapshot.output_partitions,
      snapshot.output_rows,
      snapshot.rows_written,
      format_elapsed_debug(Duration::from_nanos(snapshot.elapsed_compute_nanos)),
      snapshot.spill_count,
      format_bytes(snapshot.spilled_bytes),
    );
  }
}

/// Traverse a physical plan and snapshot metrics for each operator.
fn collect_operator_metric_snapshots(
  plan: &dyn ExecutionPlan,
  depth: usize,
  snapshots: &mut Vec<OperatorMetricSnapshot>,
) {
  let mut output_rows = 0;
  let mut rows_written = 0;
  let mut spill_count = 0;
  let mut spilled_bytes = 0;
  let mut elapsed_compute_nanos = 0;

  if let Some(plan_metrics) = plan.metrics() {
    output_rows = plan_metrics.output_rows().unwrap_or(0) as u64;
    rows_written = plan_metrics
      .sum_by_name(SINK_ROWS_METRIC)
      .map(|value| value.as_usize() as u64)
      .unwrap_or(0);
    spill_count = plan_metrics.spill_count().unwrap_or(0) as u64;
    spilled_bytes = plan_metrics.spilled_bytes().unwrap_or(0) as u64;
    elapsed_compute_nanos = plan_metrics.elapsed_compute().unwrap_or(0) as u64;
  }

  snapshots.push(OperatorMetricSnapshot {
    operator: plan.name().to_string(),
    depth,
    output_partitions: plan.output_partitioning().partition_count(),
    output_rows,
    rows_written,
    spill_count,
    spilled_bytes,
    elapsed_compute_nanos,
  });

  for child in plan.children() {
    collect_operator_metric_snapshots(child.as_ref(), depth + 1, snapshots);
  }
}

/// Print one stage duration when explain mode is enabled.
fn explain_timing(explain: bool, label: &str, elapsed: Duration) {
  if !explain {
    return;
  }
  eprintln!("[timing] {label}: {}", format_elapsed_debug(elapsed));
}

/// Format a user-facing duration with units selected for its magnitude.
fn format_elapsed(duration: Duration) -> String {
  let total_seconds = duration.as_secs();
  let hours = total_seconds / 3600;
  let minutes = (total_seconds % 3600) / 60;
  let seconds = total_seconds % 60;
  if hours > 0 {
    format!("{hours:02}:{minutes:02}:{seconds:02}")
  } else {
    format!("{minutes:02}:{seconds:02}")
  }
}

/// Format a high-precision duration for explain-mode operator metrics.
fn format_elapsed_debug(duration: Duration) -> String {
  if duration >= Duration::from_secs(1) {
    return format_elapsed(duration);
  }

  if duration >= Duration::from_millis(1) {
    return format!("{:.2}ms", duration.as_secs_f64() * 1_000.0);
  }

  if duration >= Duration::from_micros(1) {
    return format!("{:.2}µs", duration.as_secs_f64() * 1_000_000.0);
  }

  format!("{}ns", duration.as_nanos())
}

/// Format byte counts with binary units for spill diagnostics.
fn format_bytes(bytes: u64) -> String {
  const KIB: u64 = 1024;
  const MIB: u64 = 1024 * KIB;
  const GIB: u64 = 1024 * MIB;

  if bytes >= GIB {
    format!("{:.1} GiB", bytes as f64 / GIB as f64)
  } else if bytes >= MIB {
    format!("{:.1} MiB", bytes as f64 / MIB as f64)
  } else if bytes >= KIB {
    format!("{:.1} KiB", bytes as f64 / KIB as f64)
  } else {
    format!("{bytes} B")
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use arrow_schema::{DataType, Field, Schema};
  use datafusion::common::tree_node::{TreeNode, TreeNodeRecursion};
  use datafusion::physical_plan::ExecutionPlanProperties;

  #[test]
  fn formats_scan_progress_message() {
    assert_eq!(
      format_metric_progress_message(
        "Encoding display payload and writing parquet",
        "sorting buffered rows",
        PlanProgressMetrics {
          rows_read: 128,
          ..Default::default()
        },
        1024,
        true,
      ),
      "Encoding display payload and writing parquet"
    );
  }

  #[test]
  fn formats_post_read_activity_message() {
    assert_eq!(
      format_metric_progress_message(
        "Encoding display payload and writing parquet",
        "sorting buffered rows",
        PlanProgressMetrics {
          rows_read: 1024,
          elapsed_compute_nanos: Duration::from_secs(12).as_nanos() as u64,
          ..Default::default()
        },
        1024,
        true,
      ),
      "Encoding display payload and writing parquet — sorting buffered rows in memory — compute 00:12"
    );
  }

  #[test]
  fn formats_sink_side_progress_message() {
    assert_eq!(
      format_metric_progress_message(
        "Encoding display payload and writing parquet",
        "sorting buffered rows",
        PlanProgressMetrics {
          rows_read: 1024,
          rows_written: 256,
          ..Default::default()
        },
        1024,
        true,
      ),
      "Encoding display payload and writing parquet — writing 256/1024 rows (25%)"
    );
  }

  #[test]
  fn groups_write_stage_labels_for_single_file_output() {
    assert_eq!(
      write_stage_message(WriteStagePhase::Reading, false),
      "Preparing output — reading rows into sort buffers"
    );
    assert_eq!(
      write_stage_message(WriteStagePhase::Sorting, false),
      "Preparing output — sorting buffered rows"
    );
    assert_eq!(
      write_stage_message(WriteStagePhase::Writing, false),
      "Writing parquet file"
    );
  }

  #[test]
  fn groups_write_stage_labels_for_multi_file_output() {
    assert_eq!(
      write_stage_message(WriteStagePhase::Reading, true),
      "Preparing output — reading rows into sort buffers for multiple files"
    );
    assert_eq!(
      write_stage_message(WriteStagePhase::Sorting, true),
      "Preparing output — sorting rows for multiple files"
    );
    assert_eq!(
      write_stage_message(WriteStagePhase::Writing, true),
      "Writing parquet files"
    );
  }

  #[test]
  fn preserves_partitioned_sort_for_multi_file_writes() {
    let batch = RecordBatch::try_new(
      Arc::new(Schema::new(vec![
        Field::new(POINT_Z_CODE_COLUMN, DataType::UInt64, false),
        Field::new(POINT_RANGE_COLUMN, DataType::UInt64, false),
      ])),
      vec![
        Arc::new(UInt64Array::from(vec![4_u64, 1, 3, 2])),
        Arc::new(UInt64Array::from(vec![10_u64, 0, 10, 0])),
      ],
    )
    .unwrap();

    tokio::runtime::Runtime::new().unwrap().block_on(async {
      let session = new_datafusion_session().unwrap();
      let df = session.context().read_batch(batch).unwrap();

      let physical_plan = df.create_physical_plan().await.unwrap();
      assert_eq!(physical_plan.output_partitioning().partition_count(), 1);

      let rewritten = preserve_partitioned_sort_execs(
        physical_plan,
        Some(&PartitionedWriteConfig {
          partition_column: POINT_RANGE_COLUMN.to_string(),
          sort_column: POINT_Z_CODE_COLUMN.to_string(),
          bucket_count: 2,
          drop_sort_column_after_sort: false,
        }),
      )
      .unwrap();
      assert!(rewritten.output_partitioning().partition_count() > 1);

      let mut saw_sort = false;
      let mut saw_repartition = false;
      rewritten
        .apply(|plan| {
          if let Some(sort) = plan.as_any().downcast_ref::<SortExec>() {
            saw_sort = true;
            assert!(sort.preserve_partitioning());
          }
          if let Some(repartition) = plan.as_any().downcast_ref::<RepartitionExec>() {
            saw_repartition = true;
            assert!(
              matches!(repartition.partitioning(), Partitioning::Hash(_, partition_count) if *partition_count > 1)
            );
          }
          Ok(TreeNodeRecursion::Continue)
        })
        .unwrap();
      assert!(saw_sort, "expected rewritten plan to contain a sort");
      assert!(saw_repartition, "expected rewritten plan to contain a repartition");
    });
  }
}

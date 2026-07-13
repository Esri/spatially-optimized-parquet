//! Prepares spatial context, analysis, projections, and metadata for optimized writing.

use std::time::Duration;

use anyhow::{Result, bail};
use parquet::file::metadata::KeyValue;

use crate::analysis::{DisplayGeometryType, DisplayJobAnalysis, GeometryFamily};
use crate::diagnostics::{explain_stage_note, explain_timing};
use crate::geometry::GeometrySpec;
use crate::input::materialized::input_dataframe_for_job;
use crate::metadata::source::SourceDatasetMetadata;
use crate::output::geoparquet::resolve_source_context;
use crate::output::reprojection::ReprojectionPlan;
use crate::progress::{finish_row_bar, row_bar};

use super::OptimizeOutputRequest;
use super::analysis::{
  analyze_display_dataframe, analyze_helper_dataframe, metadata_display_analysis,
};
use super::dataframe::{
  OrderedDataframeRequest, build_final_projection_expressions,
  build_narrow_helper_projection_dataframe, prepare_spatially_ordered_dataframe,
};
use super::metadata::build_optimized_metadata;
use super::multiscale::{TEMP_REPROJECTED_GEOMETRY_COLUMN, create_geometry_encodings};
use super::plan::{partition_column_name, sort_column_name, validate_partition_column};

/// Stores source geometry and reprojection decisions shared by optimization stages.
pub(crate) struct SpatialPlanningContext {
  pub(crate) source_metadata: SourceDatasetMetadata,
  pub(crate) geometry_spec: GeometrySpec,
  pub(crate) reprojection: ReprojectionPlan,
  pub(crate) geometry_type: DisplayGeometryType,
}

/// Carries the final DataFrame and physical output controls into the writer stage.
pub(crate) struct PreparedOptimizeOutput {
  pub(crate) dataframe: engine::DataFrame,
  pub(crate) analysis: DisplayJobAnalysis,
  pub(crate) kv_metadata: Vec<KeyValue>,
  pub(crate) partition_column: Option<&'static str>,
  pub(crate) retained_sort_column: Option<&'static str>,
}

/// Resolve source geometry, CRS, and reprojection decisions.
pub(crate) async fn build_spatial_planning_context(
  request: &OptimizeOutputRequest<'_>,
) -> Result<SpatialPlanningContext> {
  let source_context = resolve_source_context(
    request.input,
    request.source_schema,
    request.geometry_column,
    request.input_wkid,
    request.row_range,
  )
  .await?;
  let reprojection = ReprojectionPlan::from_source_metadata(
    &source_context.source_metadata,
    &source_context.geometry_spec.column,
    request.output_wkid,
  )?;
  Ok(SpatialPlanningContext {
    source_metadata: source_context.source_metadata,
    geometry_spec: source_context.geometry_spec,
    reprojection,
    geometry_type: source_context.geometry_type,
  })
}

/// Analyze geometry and construct the final optimized output DataFrame.
pub(crate) async fn prepare_optimized_output(
  request: &OptimizeOutputRequest<'_>,
  planning: &SpatialPlanningContext,
) -> Result<PreparedOptimizeOutput> {
  let analysis = analyze_optimized_geometry(request, planning).await?;
  ensure_supported(&analysis)?;

  let encodings = match analysis.geometry_family {
    GeometryFamily::Point => Vec::new(),
    GeometryFamily::NonPoint => {
      create_geometry_encodings(request.output_wkid, analysis.geometry_type)?
    }
  };
  let partition_column =
    (request.output_layout.parts > 1).then_some(partition_column_name(&analysis));
  validate_partition_column(request.source_schema, partition_column)?;
  let ordered_request = OrderedDataframeRequest {
    input: request.input,
    session: request.session,
    source_schema: request.source_schema,
    row_range: request.row_range,
    materialized_batches: request.materialized_batches,
    output_parts: request.output_layout.parts,
    total_input_rows: request.total_input_rows,
    progress: request.progress,
    explain: request.explain,
    geometry_spec: &planning.geometry_spec,
    geometry_type: planning.geometry_type,
    transform: planning.reprojection.transform(),
  };
  let prepared_dataframe = prepare_spatially_ordered_dataframe(&ordered_request, &analysis).await?;
  let retained_sort_column =
    if partition_column.is_some() && matches!(analysis.geometry_family, GeometryFamily::NonPoint) {
      Some(sort_column_name(&analysis))
    } else {
      None
    };
  let dataframe = prepared_dataframe.select(build_final_projection_expressions(
    request.source_schema,
    &analysis,
    &encodings,
    partition_column,
    retained_sort_column,
    planning
      .reprojection
      .requires_reprojection()
      .then_some(TEMP_REPROJECTED_GEOMETRY_COLUMN),
    request.covering,
  ))?;
  let kv_metadata = build_optimized_metadata(
    &planning.source_metadata,
    &analysis,
    &encodings,
    request.covering,
  )?;

  Ok(PreparedOptimizeOutput {
    dataframe,
    analysis,
    kv_metadata,
    partition_column,
    retained_sort_column,
  })
}

async fn analyze_optimized_geometry(
  request: &OptimizeOutputRequest<'_>,
  planning: &SpatialPlanningContext,
) -> Result<DisplayJobAnalysis> {
  let analysis_bar = row_bar(
    request.progress,
    "Analyzing geometry",
    request.total_input_rows,
  );
  let analysis = if let Some(analysis) = metadata_display_analysis(
    &planning.geometry_spec,
    &planning.source_metadata,
    planning.geometry_type,
    planning.reprojection.target_spatial_reference().clone(),
    request.row_range.is_full() && !planning.reprojection.requires_reprojection(),
  ) {
    explain_stage_note(
      request.explain,
      "Analyzing geometry",
      "using metadata fast path from source metadata",
    );
    explain_timing(request.explain, "Analyzing geometry", Duration::ZERO);
    analysis_bar.inc(request.total_input_rows);
    analysis
  } else if request.output_layout.parts > 1 {
    let helper_dataframe = build_narrow_helper_projection_dataframe(
      input_dataframe_for_job(
        request.input,
        request.session,
        request.row_range,
        request.materialized_batches,
      )
      .await?,
      &planning.geometry_spec,
      planning.geometry_type,
      planning.reprojection.transform(),
    )?;
    analyze_helper_dataframe(
      helper_dataframe,
      &planning.geometry_spec,
      &planning.source_metadata,
      planning.geometry_type,
      planning.reprojection.target_spatial_reference().clone(),
      &analysis_bar,
      request.total_input_rows,
      request.explain,
    )
    .await?
  } else {
    analyze_display_dataframe(
      input_dataframe_for_job(
        request.input,
        request.session,
        request.row_range,
        request.materialized_batches,
      )
      .await?,
      &planning.geometry_spec,
      &planning.source_metadata,
      planning.geometry_type,
      planning.reprojection.transform(),
      planning.reprojection.target_spatial_reference().clone(),
      &analysis_bar,
      request.total_input_rows,
      request.explain,
    )
    .await?
  };
  finish_row_bar(
    &analysis_bar,
    request.total_input_rows,
    format!("Analyzed {} geometry", analysis.geometry_type.as_str()),
  );
  Ok(analysis)
}

/// Reject geometry categories and dimensions not implemented by display encoding.
pub(crate) fn ensure_supported(analysis: &DisplayJobAnalysis) -> Result<()> {
  if analysis.has_z || analysis.has_m {
    bail!("display optimization does not yet support Z/M geometries")
  }
  if matches!(analysis.geometry_type, DisplayGeometryType::MultiPoint) {
    bail!("display optimization does not yet support multipoint geometries")
  }
  Ok(())
}

use anyhow::{Context, Result};
use futures_util::StreamExt;

use super::derive::{
  metadata_display_geometry_type, metadata_fast_path_analysis, resolve_spatial_reference_info,
  scan_geometry_array,
};
use super::types::{DisplayJobAnalysis, SpatialReferenceInfo};
use crate::geometry::GeometrySpec;
use crate::input::{InputSource, RowRange};
use crate::metadata::source::SourceDatasetMetadata;
use crate::reprojection::TransformSpec;

/// Analyze the complete input without progress callbacks or reprojection.
pub async fn analyze_display_job(
  input: &dyn InputSource,
  geometry_spec: &GeometrySpec,
  source_metadata: &SourceDatasetMetadata,
) -> Result<DisplayJobAnalysis> {
  analyze_display_job_with_progress_and_transform(
    input,
    geometry_spec,
    source_metadata,
    RowRange::default(),
    None,
    None,
    |_| {},
  )
  .await
}

/// Analyze an optional leading row subset while reporting scanned rows.
pub async fn analyze_display_job_with_progress(
  input: &dyn InputSource,
  geometry_spec: &GeometrySpec,
  source_metadata: &SourceDatasetMetadata,
  limit_rows: Option<u64>,
  mut on_rows_scanned: impl FnMut(u64),
) -> Result<DisplayJobAnalysis> {
  analyze_display_job_with_progress_and_transform(
    input,
    geometry_spec,
    source_metadata,
    RowRange {
      start: 0,
      num: limit_rows.map(|limit| limit as usize),
    },
    None,
    None,
    |rows| on_rows_scanned(rows),
  )
  .await
}

/// Analyze selected rows and optionally evaluate extents after coordinate transformation.
///
/// Complete metadata avoids a scan only when no row limit or transform invalidates the
/// source-wide geometry facts.
pub async fn analyze_display_job_with_progress_and_transform(
  input: &dyn InputSource,
  geometry_spec: &GeometrySpec,
  source_metadata: &SourceDatasetMetadata,
  row_range: RowRange,
  geometry_transform: Option<&TransformSpec>,
  output_spatial_reference: Option<SpatialReferenceInfo>,
  mut on_rows_scanned: impl FnMut(u64),
) -> Result<DisplayJobAnalysis> {
  let source_geometry = source_metadata
    .geometry
    .as_ref()
    .filter(|geometry| geometry.column == geometry_spec.column);
  if let Some(analysis) = metadata_fast_path_analysis(
    geometry_spec,
    source_geometry,
    row_range,
    geometry_transform,
  )? {
    on_rows_scanned(row_range.effective_rows(input.total_rows()?));
    return Ok(analysis);
  }

  let mut observed_type = source_geometry.and_then(metadata_display_geometry_type);
  let mut observed_extent = if geometry_transform.is_none() {
    source_geometry.and_then(|geometry| geometry.bbox)
  } else {
    None
  };
  let mut remaining_rows = row_range.num.map(|num| num as u64);
  let mut stream = input.read_batches(row_range).await?;
  while let Some(batch) = stream.next().await {
    if remaining_rows == Some(0) {
      break;
    }
    let batch = batch?;
    let rows_to_scan = remaining_rows
      .map(|remaining| remaining.min(batch.num_rows() as u64) as usize)
      .unwrap_or(batch.num_rows());
    let array = batch
      .column_by_name(&geometry_spec.column)
      .with_context(|| format!("missing geometry column '{}'", geometry_spec.column))?;
    scan_geometry_array(
      array,
      rows_to_scan,
      &mut observed_type,
      &mut observed_extent,
      geometry_transform,
      &mut on_rows_scanned,
    )?;
    if let Some(remaining) = remaining_rows.as_mut() {
      *remaining = remaining.saturating_sub(rows_to_scan as u64);
    }
  }

  let geometry_type = observed_type.context("unable to determine display geometry type")?;
  let full_extent = observed_extent.context("unable to determine dataset full extent")?;
  let (has_z, has_m) = source_geometry
    .map(|geometry| (geometry.has_z, geometry.has_m))
    .unwrap_or((false, false));
  let projjson = source_geometry.and_then(|geometry| geometry.projjson.clone());
  let spatial_reference =
    output_spatial_reference.unwrap_or(resolve_spatial_reference_info(projjson)?);

  Ok(DisplayJobAnalysis {
    geometry_spec: geometry_spec.clone(),
    geometry_family: geometry_type.family(),
    geometry_type,
    full_extent,
    spatial_reference,
    has_z,
    has_m,
  })
}

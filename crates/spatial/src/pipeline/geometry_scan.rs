//! Scans selected WKB values for exact geometry kinds and extents.

use futures_util::StreamExt;

use crate::geometry::{Extent2D, GeometryArray, GeometryKind, WkbHeader};
use crate::pipeline::PipelineError;

pub(super) async fn scan_geometry_metadata(
  dataframe: datafusion::dataframe::DataFrame,
  geometry_column: &str,
) -> Result<(Vec<GeometryKind>, Extent2D), PipelineError> {
  let mut geometry_types = Vec::new();
  let mut full_extent: Option<Extent2D> = None;
  let mut stream = dataframe
    .select_columns(&[geometry_column])
    .map_err(|source| PipelineError::DataFusion {
      operation: "select geometry metadata column",
      source,
    })?
    .execute_stream()
    .await
    .map_err(|source| PipelineError::DataFusion {
      operation: "scan selected geometry metadata",
      source,
    })?;
  while let Some(batch) = stream.next().await {
    let batch = batch.map_err(|source| PipelineError::DataFusion {
      operation: "read geometry metadata batch",
      source,
    })?;
    let array = batch.column_by_name(geometry_column).ok_or_else(|| {
      PipelineError::InvalidRequest(format!("missing geometry column '{geometry_column}'"))
    })?;
    let geometry =
      GeometryArray::try_new(array.as_ref()).map_err(|source| PipelineError::DataFusion {
        operation: "read geometry metadata array",
        source,
      })?;
    scan_binary_values(&geometry, &mut geometry_types, &mut full_extent)?;
  }
  if geometry_types.is_empty() {
    return Err(PipelineError::InvalidRequest(
      "unable to determine geometry type from selected rows".to_string(),
    ));
  }
  Ok((
    geometry_types,
    full_extent.ok_or_else(|| {
      PipelineError::InvalidRequest(
        "unable to determine geometry extent from selected rows".to_string(),
      )
    })?,
  ))
}

fn scan_binary_values(
  geometry: &GeometryArray<'_>,
  geometry_types: &mut Vec<GeometryKind>,
  full_extent: &mut Option<Extent2D>,
) -> Result<(), PipelineError> {
  for value in geometry.values() {
    let Some(bytes) = value else {
      continue;
    };
    let geometry_kind = WkbHeader::read(bytes)?.kind;
    if !geometry_types.contains(&geometry_kind) {
      geometry_types.push(geometry_kind);
    }
    let extent = Extent2D::from_wkb(bytes)?;
    match full_extent {
      Some(full_extent) => {
        full_extent.xmin = full_extent.xmin.min(extent.xmin);
        full_extent.ymin = full_extent.ymin.min(extent.ymin);
        full_extent.xmax = full_extent.xmax.max(extent.xmax);
        full_extent.ymax = full_extent.ymax.max(extent.ymax);
      }
      None => *full_extent = Some(extent),
    }
  }
  Ok(())
}

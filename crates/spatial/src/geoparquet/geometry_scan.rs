//! Scans selected WKB values for exact geometry kinds and extents.

use anyhow::{Context, Result, bail};
use futures_util::StreamExt;

use crate::geometry::{Extent2D, GeometryArray, GeometryKind, geometry_kind_from_wkb};
use crate::optimized::geometry_extent_from_wkb;

pub(super) async fn scan_geometry_metadata(
  dataframe: datafusion::dataframe::DataFrame,
  geometry_column: &str,
) -> Result<(Vec<GeometryKind>, Extent2D)> {
  let mut geometry_types = Vec::new();
  let mut full_extent: Option<Extent2D> = None;
  let mut stream = dataframe
    .select_columns(&[geometry_column])?
    .execute_stream()
    .await
    .context("scan selected geometry metadata")?;
  while let Some(batch) = stream.next().await {
    let batch = batch?;
    let array = batch
      .column_by_name(geometry_column)
      .with_context(|| format!("missing geometry column '{geometry_column}'"))?;
    let geometry = GeometryArray::try_new(array.as_ref()).map_err(anyhow::Error::from)?;
    scan_binary_values(&geometry, &mut geometry_types, &mut full_extent)?;
  }
  if geometry_types.is_empty() {
    bail!("unable to determine geometry type from selected rows");
  }
  Ok((
    geometry_types,
    full_extent.context("unable to determine geometry extent from selected rows")?,
  ))
}

fn scan_binary_values(
  geometry: &GeometryArray<'_>,
  geometry_types: &mut Vec<GeometryKind>,
  full_extent: &mut Option<Extent2D>,
) -> Result<()> {
  for value in geometry.values() {
    let Some(bytes) = value else {
      continue;
    };
    let geometry_kind = geometry_kind_from_wkb(bytes)?;
    if !geometry_types.contains(&geometry_kind) {
      geometry_types.push(geometry_kind);
    }
    let extent = geometry_extent_from_wkb(bytes)?;
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

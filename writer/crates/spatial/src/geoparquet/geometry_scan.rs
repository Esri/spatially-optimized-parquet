//! Scans selected WKB values for exact geometry kinds and extents.

use anyhow::{Context, Result, bail};
use futures_util::StreamExt;

use crate::geometry::{BinaryValueAccess, Extent2D, GeometryKind, geometry_kind_from_wkb};
use crate::optimized::multiscale::geometry_extent_from_wkb;

pub(crate) async fn scan_geometry_metadata(
  dataframe: engine::DataFrame,
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
    match array.data_type() {
      arrow_schema::DataType::Binary => scan_binary_values(
        array
          .as_any()
          .downcast_ref::<arrow_array::BinaryArray>()
          .context("geometry column was not Binary")?,
        &mut geometry_types,
        &mut full_extent,
      )?,
      arrow_schema::DataType::LargeBinary => scan_binary_values(
        array
          .as_any()
          .downcast_ref::<arrow_array::LargeBinaryArray>()
          .context("geometry column was not LargeBinary")?,
        &mut geometry_types,
        &mut full_extent,
      )?,
      arrow_schema::DataType::BinaryView => scan_binary_values(
        array
          .as_any()
          .downcast_ref::<arrow_array::BinaryViewArray>()
          .context("geometry column was not BinaryView")?,
        &mut geometry_types,
        &mut full_extent,
      )?,
      data_type => bail!("unsupported geometry column type: {data_type}"),
    }
  }
  if geometry_types.is_empty() {
    bail!("unable to determine geometry type from selected rows");
  }
  Ok((
    geometry_types,
    full_extent.context("unable to determine geometry extent from selected rows")?,
  ))
}

fn scan_binary_values<T: BinaryValueAccess>(
  array: &T,
  geometry_types: &mut Vec<GeometryKind>,
  full_extent: &mut Option<Extent2D>,
) -> Result<()> {
  for index in 0..array.len() {
    let Some(bytes) = array.value_opt(index) else {
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

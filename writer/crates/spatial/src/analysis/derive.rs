use anyhow::{Context, Result};
use arrow_array::{Array, ArrayRef, BinaryArray, BinaryViewArray, LargeBinaryArray};
use arrow_schema::DataType;
use serde_json::Value;

use super::{DisplayGeometryType, DisplayJobAnalysis, Extent2D, SpatialReferenceInfo};
use crate::geometry::{GeometryKind, GeometrySpec, geometry_kind_from_wkb_type};
use crate::input::RowRange;
use crate::metadata::source::SourceGeometryMetadata;
use crate::output::optimized::multiscale::geometry_extent_from_trait;
use crate::reprojection::TransformSpec;

const ANALYSIS_PROGRESS_MAX_CHUNK_ROWS: usize = 8_192;
const ANALYSIS_PROGRESS_TARGET_UPDATES_PER_BATCH: usize = 32;

pub(super) fn metadata_fast_path_analysis(
  geometry_spec: &GeometrySpec,
  source_geometry: Option<&SourceGeometryMetadata>,
  row_range: RowRange,
  geometry_transform: Option<&TransformSpec>,
) -> Result<Option<DisplayJobAnalysis>> {
  if !row_range.is_full() || geometry_transform.is_some() {
    return Ok(None);
  }
  let Some(source_geometry) = source_geometry else {
    return Ok(None);
  };
  let Some(geometry_type) = metadata_display_geometry_type(source_geometry) else {
    return Ok(None);
  };
  let Some(full_extent) = source_geometry.bbox else {
    return Ok(None);
  };
  let spatial_reference = resolve_spatial_reference_info(source_geometry.projjson.clone())?;
  Ok(Some(DisplayJobAnalysis {
    geometry_spec: geometry_spec.clone(),
    geometry_family: geometry_type.family(),
    geometry_type,
    full_extent,
    spatial_reference,
    has_z: source_geometry.has_z,
    has_m: source_geometry.has_m,
  }))
}

pub(super) fn scan_geometry_array<F: FnMut(u64)>(
  array: &ArrayRef,
  max_rows: usize,
  observed_type: &mut Option<DisplayGeometryType>,
  observed_extent: &mut Option<Extent2D>,
  geometry_transform: Option<&TransformSpec>,
  on_rows_scanned: &mut F,
) -> Result<()> {
  match array.data_type() {
    DataType::Binary => scan_binary(
      array
        .as_any()
        .downcast_ref::<BinaryArray>()
        .context("binary geometry array")?,
      max_rows,
      observed_type,
      observed_extent,
      geometry_transform,
      on_rows_scanned,
    ),
    DataType::LargeBinary => scan_large_binary(
      array
        .as_any()
        .downcast_ref::<LargeBinaryArray>()
        .context("large binary geometry array")?,
      max_rows,
      observed_type,
      observed_extent,
      geometry_transform,
      on_rows_scanned,
    ),
    DataType::BinaryView => scan_binary_view(
      array
        .as_any()
        .downcast_ref::<BinaryViewArray>()
        .context("binary view geometry array")?,
      max_rows,
      observed_type,
      observed_extent,
      geometry_transform,
      on_rows_scanned,
    ),
    other => Err(anyhow::anyhow!("unsupported geometry data type: {other}")),
  }
}

fn scan_binary<F: FnMut(u64)>(
  array: &BinaryArray,
  max_rows: usize,
  observed_type: &mut Option<DisplayGeometryType>,
  observed_extent: &mut Option<Extent2D>,
  geometry_transform: Option<&TransformSpec>,
  on_rows_scanned: &mut F,
) -> Result<()> {
  let chunk_rows = analysis_progress_chunk_rows(max_rows);
  let mut pending_rows = 0u64;
  for index in 0..array.len().min(max_rows) {
    if !array.is_null(index) {
      observe_geometry(
        array.value(index),
        observed_type,
        observed_extent,
        geometry_transform,
      )?;
    }
    pending_rows += 1;
    if pending_rows as usize >= chunk_rows {
      on_rows_scanned(pending_rows);
      pending_rows = 0;
    }
  }
  if pending_rows > 0 {
    on_rows_scanned(pending_rows);
  }
  Ok(())
}

fn scan_large_binary<F: FnMut(u64)>(
  array: &LargeBinaryArray,
  max_rows: usize,
  observed_type: &mut Option<DisplayGeometryType>,
  observed_extent: &mut Option<Extent2D>,
  geometry_transform: Option<&TransformSpec>,
  on_rows_scanned: &mut F,
) -> Result<()> {
  let chunk_rows = analysis_progress_chunk_rows(max_rows);
  let mut pending_rows = 0u64;
  for index in 0..array.len().min(max_rows) {
    if !array.is_null(index) {
      observe_geometry(
        array.value(index),
        observed_type,
        observed_extent,
        geometry_transform,
      )?;
    }
    pending_rows += 1;
    if pending_rows as usize >= chunk_rows {
      on_rows_scanned(pending_rows);
      pending_rows = 0;
    }
  }
  if pending_rows > 0 {
    on_rows_scanned(pending_rows);
  }
  Ok(())
}

fn scan_binary_view<F: FnMut(u64)>(
  array: &BinaryViewArray,
  max_rows: usize,
  observed_type: &mut Option<DisplayGeometryType>,
  observed_extent: &mut Option<Extent2D>,
  geometry_transform: Option<&TransformSpec>,
  on_rows_scanned: &mut F,
) -> Result<()> {
  let chunk_rows = analysis_progress_chunk_rows(max_rows);
  let mut pending_rows = 0u64;
  for index in 0..array.len().min(max_rows) {
    if array.is_valid(index) {
      observe_geometry(
        array.value(index),
        observed_type,
        observed_extent,
        geometry_transform,
      )?;
    }
    pending_rows += 1;
    if pending_rows as usize >= chunk_rows {
      on_rows_scanned(pending_rows);
      pending_rows = 0;
    }
  }
  if pending_rows > 0 {
    on_rows_scanned(pending_rows);
  }
  Ok(())
}

fn analysis_progress_chunk_rows(max_rows: usize) -> usize {
  let target_chunk = max_rows / ANALYSIS_PROGRESS_TARGET_UPDATES_PER_BATCH;
  target_chunk.clamp(1, ANALYSIS_PROGRESS_MAX_CHUNK_ROWS)
}

fn observe_geometry(
  bytes: &[u8],
  observed_type: &mut Option<DisplayGeometryType>,
  observed_extent: &mut Option<Extent2D>,
  geometry_transform: Option<&TransformSpec>,
) -> Result<()> {
  let geometry = wkb::reader::read_wkb(bytes)?;
  let kind = geometry_kind_from_wkb_type(geometry.geometry_type());
  let display_type = map_kind_to_display_type(kind)
    .with_context(|| format!("unsupported display geometry kind: {kind:?}"))?;
  match observed_type {
    Some(existing) if *existing != display_type => {
      return Err(anyhow::anyhow!(
        "mixed display geometry types are unsupported: saw {existing:?} and {display_type:?}"
      ));
    }
    Some(_) => {}
    None => *observed_type = Some(display_type),
  }

  let extent = if let Some(transform) = geometry_transform {
    transform.transform_geometry_bounds_from_wkb(bytes, display_type)?
  } else {
    geometry_extent_from_trait(&geometry).context("geometry missing bounding rectangle")?
  };
  match observed_extent {
    Some(existing) => existing.expand_to_include(&extent),
    None => *observed_extent = Some(extent),
  }

  Ok(())
}

fn map_kind_to_display_type(kind: GeometryKind) -> Option<DisplayGeometryType> {
  match kind {
    GeometryKind::Point => Some(DisplayGeometryType::Point),
    GeometryKind::MultiPoint => Some(DisplayGeometryType::MultiPoint),
    GeometryKind::LineString | GeometryKind::MultiLineString => Some(DisplayGeometryType::Polyline),
    GeometryKind::Polygon | GeometryKind::MultiPolygon => Some(DisplayGeometryType::Polygon),
    GeometryKind::GeometryCollection | GeometryKind::Unknown => None,
  }
}

pub(super) fn metadata_display_geometry_type(
  geometry_meta: &SourceGeometryMetadata,
) -> Option<DisplayGeometryType> {
  let mut geometry_type = None;
  for geometry_kind in &geometry_meta.geometry_types {
    let candidate = match geometry_kind {
      GeometryKind::Point => DisplayGeometryType::Point,
      GeometryKind::MultiPoint => DisplayGeometryType::MultiPoint,
      GeometryKind::LineString | GeometryKind::MultiLineString => DisplayGeometryType::Polyline,
      GeometryKind::Polygon | GeometryKind::MultiPolygon => DisplayGeometryType::Polygon,
      GeometryKind::GeometryCollection | GeometryKind::Unknown => return None,
    };
    match geometry_type {
      Some(existing) if existing != candidate => return None,
      Some(_) => {}
      None => geometry_type = Some(candidate),
    }
  }
  geometry_type
}

pub(super) fn resolve_spatial_reference_info(
  projjson: Option<Value>,
) -> Result<SpatialReferenceInfo> {
  Ok(SpatialReferenceInfo {
    wkid: projjson.as_ref().and_then(infer_wkid),
    wkt: None,
    projjson,
  })
}

fn infer_wkid(value: &Value) -> Option<u32> {
  match value {
    Value::Object(map) => {
      if let Some(id_value) = map.get("id") {
        return authority_code(id_value);
      }
      authority_code(value)
    }
    _ => None,
  }
}

fn authority_code(value: &Value) -> Option<u32> {
  let Value::Object(map) = value else {
    return None;
  };
  let authority = map.get("authority").and_then(Value::as_str);
  let code = map.get("code").and_then(value_as_u32);
  matches!(authority, Some("EPSG" | "ESRI"))
    .then_some(code)
    .flatten()
}

fn value_as_u32(value: &Value) -> Option<u32> {
  value
    .as_u64()
    .and_then(|value| u32::try_from(value).ok())
    .or_else(|| value.as_str().and_then(|value| value.parse::<u32>().ok()))
}

use anyhow::{Context, Result};
use arrow_array::{Array, ArrayRef, BinaryArray, BinaryViewArray, LargeBinaryArray};
use arrow_schema::DataType;
use futures_util::StreamExt;
use serde::Serialize;
use serde_json::Value;

use crate::geometry::{GeometryKind, GeometrySpec, geometry_kind_from_wkb_type};
use crate::input::{InputSource, RowRange};
use crate::metadata::source::{SourceDatasetMetadata, SourceGeometryMetadata};
use crate::pbf::geometry_extent_from_trait;
use crate::reprojection::TransformSpec;

const ANALYSIS_PROGRESS_MAX_CHUNK_ROWS: usize = 8_192;
const ANALYSIS_PROGRESS_TARGET_UPDATES_PER_BATCH: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeometryFamily {
  Point,
  NonPoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DisplayGeometryType {
  Point,
  MultiPoint,
  Polyline,
  Polygon,
}

impl DisplayGeometryType {
  pub fn as_str(self) -> &'static str {
    match self {
      Self::Point => "point",
      Self::MultiPoint => "multipoint",
      Self::Polyline => "polyline",
      Self::Polygon => "polygon",
    }
  }

  pub fn family(self) -> GeometryFamily {
    match self {
      Self::Point => GeometryFamily::Point,
      Self::MultiPoint | Self::Polyline | Self::Polygon => GeometryFamily::NonPoint,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Default)]
pub struct Extent2D {
  pub xmin: f64,
  pub ymin: f64,
  pub xmax: f64,
  pub ymax: f64,
}

impl Extent2D {
  fn expand_to_include(&mut self, other: &Self) {
    self.xmin = self.xmin.min(other.xmin);
    self.ymin = self.ymin.min(other.ymin);
    self.xmax = self.xmax.max(other.xmax);
    self.ymax = self.ymax.max(other.ymax);
  }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SpatialReferenceInfo {
  pub wkid: Option<u32>,
  pub wkt: Option<String>,
  pub projjson: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DisplayJobAnalysis {
  pub geometry_spec: GeometrySpec,
  pub geometry_type: DisplayGeometryType,
  pub geometry_family: GeometryFamily,
  pub full_extent: Extent2D,
  pub spatial_reference: SpatialReferenceInfo,
  pub has_z: bool,
  pub has_m: bool,
}

pub(crate) fn source_display_geometry_type(
  source_metadata: &SourceDatasetMetadata,
  geometry_column: &str,
) -> Option<DisplayGeometryType> {
  source_metadata
    .geometry
    .as_ref()
    .filter(|geometry| geometry.column == geometry_column)
    .and_then(metadata_display_geometry_type)
}

pub async fn analyze_display_job(
  input: &dyn InputSource,
  geometry_spec: &GeometrySpec,
  source_metadata: &SourceDatasetMetadata,
) -> Result<DisplayJobAnalysis> {
  analyze_display_job_with_progress_and_transform(
    input,
    geometry_spec,
    source_metadata,
    None,
    None,
    None,
    |_| {},
  )
  .await
}

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
    limit_rows,
    None,
    None,
    |rows| on_rows_scanned(rows),
  )
  .await
}

pub async fn analyze_display_job_with_progress_and_transform(
  input: &dyn InputSource,
  geometry_spec: &GeometrySpec,
  source_metadata: &SourceDatasetMetadata,
  limit_rows: Option<u64>,
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
    limit_rows,
    geometry_transform,
  )? {
    on_rows_scanned(input.total_rows()?);
    return Ok(analysis);
  }
  let mut observed_type = source_geometry.and_then(metadata_display_geometry_type);
  let mut observed_extent = if geometry_transform.is_none() {
    source_geometry.and_then(|geometry| geometry.bbox)
  } else {
    None
  };
  let mut remaining_rows = limit_rows;
  let mut stream = input
    .read_batches(RowRange {
      start: 0,
      num: limit_rows.map(|limit| limit as usize),
    })
    .await?;
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

fn metadata_fast_path_analysis(
  geometry_spec: &GeometrySpec,
  source_geometry: Option<&SourceGeometryMetadata>,
  limit_rows: Option<u64>,
  geometry_transform: Option<&TransformSpec>,
) -> Result<Option<DisplayJobAnalysis>> {
  if limit_rows.is_some() || geometry_transform.is_some() {
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

fn scan_geometry_array<F: FnMut(u64)>(
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
  for i in 0..array.len().min(max_rows) {
    if !array.is_null(i) {
      observe_geometry(
        array.value(i),
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
  for i in 0..array.len().min(max_rows) {
    if !array.is_null(i) {
      observe_geometry(
        array.value(i),
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
  for idx in 0..array.len().min(max_rows) {
    if array.is_valid(idx) {
      observe_geometry(
        array.value(idx),
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

fn metadata_display_geometry_type(
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

fn resolve_spatial_reference_info(projjson: Option<Value>) -> Result<SpatialReferenceInfo> {
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
    .and_then(|v| u32::try_from(v).ok())
    .or_else(|| value.as_str().and_then(|v| v.parse::<u32>().ok()))
}

//! Resolves and serializes the GeoParquet contract shared by both output workflows.
//!
//! A valid context contains one WKB geometry column, an authoritative input CRS, exact geometry
//! kinds, and the selected-row extent. Existing source metadata supplies those facts when
//! complete. Otherwise the module scans WKB values, while `--in-sr` can supply only a missing
//! CRS and never overrides source metadata.

use anyhow::{Context, Result, bail};
use arrow_array::{Array, BinaryArray, BinaryViewArray, LargeBinaryArray};
use arrow_schema::Schema;
use futures_util::StreamExt;
use gdal::spatial_ref::{AxisMappingStrategy, SpatialRef};
use serde_json::Value;

use crate::geometry::{
  Extent2D, GeometryEncoding, GeometryKind, GeometryShape, GeometrySpec, geometry_kind_from_wkb,
};
use crate::geoparquet::metadata::source::{SourceDatasetMetadata, SourceGeometryMetadata};
use crate::input::{InputSource, RowRange};
use crate::optimized::multiscale::COVERING_BBOX_COLUMN;
use crate::optimized::multiscale::geometry_extent_from_wkb;
use crate::output::SpatialReferenceInfo;

/// Stores normalized source geometry facts required by either GeoParquet output workflow.
#[derive(Debug, Clone)]
pub struct SourceGeoParquetContext {
  /// Stores the selected WKB geometry column.
  pub geometry_spec: GeometrySpec,
  /// Stores the exact source geometry kinds.
  pub geometry_types: Vec<GeometryKind>,
  /// Stores the selected-row extent in source coordinates.
  pub source_extent: Extent2D,
  /// Stores the source coordinate reference system.
  pub source_spatial_reference: SpatialReferenceInfo,
  /// Stores the normalized geometry shape used by plain output mechanics.
  pub geometry_shape: GeometryShape,
  /// Indicates whether source metadata declares Z ordinates.
  pub has_z: bool,
  /// Indicates whether source metadata declares M ordinates.
  pub has_m: bool,
  /// Stores normalized metadata with completed geometry facts.
  pub source_metadata: SourceDatasetMetadata,
}

/// Reject covering output that would overwrite an existing source column.
pub(crate) fn validate_covering_configuration(covering: bool, schema: &Schema) -> Result<()> {
  if covering && schema.field_with_name(COVERING_BBOX_COLUMN).is_ok() {
    bail!("--covering would overwrite existing input column '{COVERING_BBOX_COLUMN}'");
  }
  Ok(())
}

/// Resolve geometry, CRS, exact type, and extent for the selected rows.
pub async fn resolve_source_context(
  input: &dyn InputSource,
  schema: &Schema,
  explicit_geometry_column: Option<&str>,
  input_wkid: Option<u32>,
  row_range: RowRange,
) -> Result<SourceGeoParquetContext> {
  let geometry_spec = resolve_geometry_spec(
    schema,
    input.inferred_geometry_spec()?,
    explicit_geometry_column,
  )?;
  let mut source_metadata = input.source_metadata()?;
  apply_input_wkid(&mut source_metadata, &geometry_spec.column, input_wkid)?;

  let source_geometry = source_metadata
    .geometry
    .as_ref()
    .filter(|geometry| geometry.column == geometry_spec.column)
    .context("missing geometry metadata after input CRS resolution")?;
  let requires_scan = !row_range.is_full()
    || source_geometry.geometry_types.is_empty()
    || source_geometry.bbox.is_none();
  let (geometry_types, source_extent) = if requires_scan {
    scan_geometry_metadata(input, &geometry_spec.column, row_range).await?
  } else {
    (
      source_geometry.geometry_types.clone(),
      source_geometry
        .bbox
        .context("missing source geometry extent")?,
    )
  };
  let geometry_shape = GeometryShape::from_kinds(&geometry_types)?;
  let projjson = source_geometry
    .projjson
    .clone()
    .context("missing input CRS metadata")?;
  let source_spatial_reference = spatial_reference_info(&projjson)?;
  let has_z = source_geometry.has_z;
  let has_m = source_geometry.has_m;

  source_metadata.geometry = Some(SourceGeometryMetadata {
    column: geometry_spec.column.clone(),
    encoding: GeometryEncoding::Wkb,
    geometry_types: geometry_types.clone(),
    bbox: Some(source_extent),
    projjson: Some(projjson),
    has_z,
    has_m,
  });

  Ok(SourceGeoParquetContext {
    geometry_spec,
    geometry_types,
    source_extent,
    source_spatial_reference,
    geometry_shape,
    has_z,
    has_m,
    source_metadata,
  })
}

fn resolve_geometry_spec(
  schema: &Schema,
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
  inferred_geometry_spec.context("unable to resolve geometry spec; pass --geometry-column")
}

fn apply_input_wkid(
  source_metadata: &mut SourceDatasetMetadata,
  geometry_column: &str,
  input_wkid: Option<u32>,
) -> Result<()> {
  let existing_geometry = source_metadata
    .geometry
    .as_ref()
    .filter(|geometry| geometry.column == geometry_column);
  if let Some(input_wkid) = input_wkid {
    if existing_geometry
      .and_then(|geometry| geometry.projjson.as_ref())
      .is_some()
    {
      bail!(
        "--in-sr cannot be used because geometry column '{geometry_column}' already has CRS metadata"
      );
    }
    let projjson = projjson_from_epsg(input_wkid)?;
    let existing = existing_geometry.cloned();
    source_metadata.geometry = Some(SourceGeometryMetadata {
      column: geometry_column.to_string(),
      encoding: GeometryEncoding::Wkb,
      geometry_types: existing
        .as_ref()
        .map(|geometry| geometry.geometry_types.clone())
        .unwrap_or_default(),
      bbox: existing.as_ref().and_then(|geometry| geometry.bbox),
      projjson: Some(projjson),
      has_z: existing.as_ref().is_some_and(|geometry| geometry.has_z),
      has_m: existing.as_ref().is_some_and(|geometry| geometry.has_m),
    });
  } else if existing_geometry
    .and_then(|geometry| geometry.projjson.as_ref())
    .is_none()
  {
    bail!(
      "missing CRS metadata for geometry column '{geometry_column}'; pass --in-sr <LATEST_WKID>"
    );
  }
  Ok(())
}

fn projjson_from_epsg(wkid: u32) -> Result<Value> {
  let mut spatial_ref =
    SpatialRef::from_epsg(wkid).with_context(|| format!("load input EPSG:{wkid}"))?;
  spatial_ref.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
  let projjson = spatial_ref
    .to_projjson()
    .with_context(|| format!("export input EPSG:{wkid} as PROJJSON"))?;
  serde_json::from_str(&projjson).context("decode input CRS PROJJSON")
}

fn spatial_reference_info(projjson: &Value) -> Result<SpatialReferenceInfo> {
  let definition = serde_json::to_string(projjson).context("serialize input CRS PROJJSON")?;
  let mut spatial_ref =
    SpatialRef::from_definition(&definition).context("load input spatial reference")?;
  spatial_ref.set_axis_mapping_strategy(AxisMappingStrategy::TraditionalGisOrder);
  Ok(SpatialReferenceInfo {
    wkid: projjson.get("id").and_then(supported_authority_code),
    wkt: spatial_ref.to_wkt().ok(),
    projjson: Some(projjson.clone()),
  })
}

fn supported_authority_code(value: &Value) -> Option<u32> {
  let authority = value.get("authority").and_then(Value::as_str);
  let code = value.get("code").and_then(value_as_u32);
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

async fn scan_geometry_metadata(
  input: &dyn InputSource,
  geometry_column: &str,
  row_range: RowRange,
) -> Result<(Vec<GeometryKind>, Extent2D)> {
  let mut geometry_types = Vec::new();
  let mut full_extent: Option<Extent2D> = None;
  let mut stream = input.read_batches(row_range).await?;
  while let Some(batch) = stream.next().await {
    let batch = batch?;
    let array = batch
      .column_by_name(geometry_column)
      .with_context(|| format!("missing geometry column '{geometry_column}'"))?;
    match array.data_type() {
      arrow_schema::DataType::Binary => scan_binary_values(
        array
          .as_any()
          .downcast_ref::<BinaryArray>()
          .context("geometry column was not Binary")?,
        &mut geometry_types,
        &mut full_extent,
      )?,
      arrow_schema::DataType::LargeBinary => scan_binary_values(
        array
          .as_any()
          .downcast_ref::<LargeBinaryArray>()
          .context("geometry column was not LargeBinary")?,
        &mut geometry_types,
        &mut full_extent,
      )?,
      arrow_schema::DataType::BinaryView => scan_binary_values(
        array
          .as_any()
          .downcast_ref::<BinaryViewArray>()
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

fn scan_binary_values<T: BinaryValues>(
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

trait BinaryValues {
  fn len(&self) -> usize;
  fn value_opt(&self, index: usize) -> Option<&[u8]>;
}

impl BinaryValues for BinaryArray {
  fn len(&self) -> usize {
    Array::len(self)
  }

  fn value_opt(&self, index: usize) -> Option<&[u8]> {
    (!self.is_null(index)).then(|| self.value(index))
  }
}

impl BinaryValues for LargeBinaryArray {
  fn len(&self) -> usize {
    Array::len(self)
  }

  fn value_opt(&self, index: usize) -> Option<&[u8]> {
    (!self.is_null(index)).then(|| self.value(index))
  }
}

impl BinaryValues for BinaryViewArray {
  fn len(&self) -> usize {
    Array::len(self)
  }

  fn value_opt(&self, index: usize) -> Option<&[u8]> {
    (!self.is_null(index)).then(|| self.value(index))
  }
}

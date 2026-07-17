//! Resolves normalized GeoParquet source facts for output workflows.

use anyhow::{Context, Result};
use arrow_schema::Schema;

use crate::geometry::{Extent2D, GeometryColumn, GeometryEncoding, GeometryKind, GeometryType};
use crate::geoparquet::geometry_scan::scan_geometry_metadata;
use crate::geoparquet::source_crs::{resolve_source_crs, spatial_reference_info};
use crate::input::{InputSource, RowRange, SourceDatasetMetadata, SourceGeometryMetadata};
use crate::output::SpatialReferenceInfo;

/// Stores normalized source geometry facts required by either GeoParquet output workflow.
#[derive(Debug, Clone)]
pub(crate) struct ResolvedGeoParquetSource {
  /// Stores the selected WKB geometry column.
  pub(crate) geometry: GeometryColumn,
  /// Stores the exact source geometry kinds.
  pub(crate) geometry_types: Vec<GeometryKind>,
  /// Stores the selected-row extent in source coordinates.
  pub(crate) source_extent: Extent2D,
  /// Stores the source coordinate reference system.
  pub(crate) source_spatial_reference: SpatialReferenceInfo,
  /// Stores the normalized geometry type used by plain output mechanics.
  pub(crate) geometry_type: GeometryType,
  /// Indicates whether source metadata declares Z values.
  pub(crate) has_z: bool,
  /// Indicates whether source metadata declares M values.
  pub(crate) has_m: bool,
  /// Stores normalized metadata with completed geometry facts.
  pub(crate) source_metadata: SourceDatasetMetadata,
}

impl ResolvedGeoParquetSource {
  pub(crate) fn strip_dimensions(&mut self, strip_z: bool, strip_m: bool) {
    self.has_z &= !strip_z;
    self.has_m &= !strip_m;
    if let Some(geometry) = self.source_metadata.geometry.as_mut()
      && geometry.column == self.geometry.column
    {
      geometry.has_z = self.has_z;
      geometry.has_m = self.has_m;
    }
  }
}

/// Resolve geometry, CRS, exact type, and extent for the selected rows.
pub(crate) async fn resolve_source(
  input: &dyn InputSource,
  input_dataframe: datafusion::dataframe::DataFrame,
  schema: &Schema,
  explicit_geometry_column: Option<&str>,
  input_wkid: Option<u32>,
  row_range: RowRange,
) -> Result<ResolvedGeoParquetSource> {
  let geometry = resolve_geometry_column(
    schema,
    input.inferred_geometry_column()?,
    explicit_geometry_column,
  )?;
  let mut source_metadata = input.source_metadata()?;
  resolve_source_crs(&mut source_metadata, &geometry.column, input_wkid)?;

  let source_geometry = source_metadata
    .geometry
    .as_ref()
    .filter(|source_geometry| source_geometry.column == geometry.column)
    .context("missing geometry metadata after input CRS resolution")?;
  let requires_scan = !row_range.is_full()
    || source_geometry.geometry_types.is_empty()
    || source_geometry.bbox.is_none();
  let (geometry_types, source_extent) = if requires_scan {
    scan_geometry_metadata(input_dataframe, &geometry.column).await?
  } else {
    (
      source_geometry.geometry_types.clone(),
      source_geometry
        .bbox
        .context("missing source geometry extent")?,
    )
  };
  let geometry_type = GeometryType::from_kinds(&geometry_types)?;
  let projjson = source_geometry
    .projjson
    .clone()
    .context("missing input CRS metadata")?;
  let source_spatial_reference = spatial_reference_info(&projjson)?;
  let has_z = source_geometry.has_z;
  let has_m = source_geometry.has_m;

  source_metadata.geometry = Some(SourceGeometryMetadata {
    column: geometry.column.clone(),
    encoding: GeometryEncoding::Wkb,
    geometry_types: geometry_types.clone(),
    bbox: Some(source_extent),
    covering: source_geometry.covering.clone(),
    projjson: Some(projjson),
    has_z,
    has_m,
  });

  Ok(ResolvedGeoParquetSource {
    geometry,
    geometry_types,
    source_extent,
    source_spatial_reference,
    geometry_type,
    has_z,
    has_m,
    source_metadata,
  })
}

fn resolve_geometry_column(
  schema: &Schema,
  inferred_geometry_column: Option<GeometryColumn>,
  explicit_geometry_column: Option<&str>,
) -> Result<GeometryColumn> {
  if let Some(column) = explicit_geometry_column {
    schema
      .field_with_name(column)
      .with_context(|| format!("missing geometry column '{column}'"))?;
    return Ok(GeometryColumn {
      column: column.to_string(),
      encoding: GeometryEncoding::Wkb,
      geometry_kind: None,
    });
  }
  inferred_geometry_column.context("unable to resolve geometry column; pass --geometry-column")
}

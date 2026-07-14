//! Resolves normalized GeoParquet source facts for output workflows.

use anyhow::{Context, Result};
use arrow_schema::Schema;

use crate::geometry::{Extent2D, GeometryEncoding, GeometryKind, GeometryShape, GeometrySpec};
use crate::geoparquet::geometry_scan::scan_geometry_metadata;
use crate::geoparquet::source_crs::{apply_input_wkid, spatial_reference_info};
use crate::input::{InputSource, RowRange, SourceDatasetMetadata, SourceGeometryMetadata};
use crate::output::SpatialReferenceInfo;

/// Stores normalized source geometry facts required by either GeoParquet output workflow.
#[derive(Debug, Clone)]
pub(crate) struct ResolvedGeoParquetSource {
  /// Stores the selected WKB geometry column.
  pub(crate) geometry_spec: GeometrySpec,
  /// Stores the exact source geometry kinds.
  pub(crate) geometry_types: Vec<GeometryKind>,
  /// Stores the selected-row extent in source coordinates.
  pub(crate) source_extent: Extent2D,
  /// Stores the source coordinate reference system.
  pub(crate) source_spatial_reference: SpatialReferenceInfo,
  /// Stores the normalized geometry shape used by plain output mechanics.
  pub(crate) geometry_shape: GeometryShape,
  /// Indicates whether source metadata declares Z ordinates.
  pub(crate) has_z: bool,
  /// Indicates whether source metadata declares M ordinates.
  pub(crate) has_m: bool,
  /// Stores normalized metadata with completed geometry facts.
  pub(crate) source_metadata: SourceDatasetMetadata,
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
    scan_geometry_metadata(input_dataframe, &geometry_spec.column).await?
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
    covering: source_geometry.covering.clone(),
    projjson: Some(projjson),
    has_z,
    has_m,
  });

  Ok(ResolvedGeoParquetSource {
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

//! Extends shared GeoParquet metadata with SOP geodisplay metadata.

use anyhow::Result;
use parquet::file::metadata::KeyValue;

use crate::analysis::{DisplayJobAnalysis, GeometryFamily};
use crate::geometry::GeometryKind;
use crate::metadata::output::{DisplayIndexXz, DisplayIndexZ, GeodisplayMetadata};
use crate::metadata::source::SourceDatasetMetadata;
use crate::output::geoparquet::{build_geo_key_values, build_geo_metadata};
use crate::output::optimized::clustering::{DEFAULT_COORDINATE_PRECISION, DEFAULT_XZ_MAX_LEVEL};
use crate::output::optimized::multiscale::{
  BOUNDS_COLUMN, COVERING_BBOX_COLUMN, DISPLAY_COLUMN, GeometryEncoding, POINT_X_COLUMN,
  POINT_Y_COLUMN, POINT_Z_CODE_COLUMN, XZ_CODE_COLUMN, metadata_levels,
};

/// Build GeoParquet and geodisplay metadata for optimized output.
pub(crate) fn build_optimized_metadata(
  source_metadata: &SourceDatasetMetadata,
  analysis: &DisplayJobAnalysis,
  encodings: &[GeometryEncoding],
  covering: bool,
) -> Result<Vec<KeyValue>> {
  let source_geometry = source_metadata
    .geometry
    .as_ref()
    .filter(|geometry| geometry.column == analysis.geometry_spec.column);
  let geometry_types = source_geometry
    .filter(|geometry| !geometry.geometry_types.is_empty())
    .map(|geometry| geometry.geometry_types.clone())
    .unwrap_or_else(|| vec![fallback_geometry_kind(analysis)]);
  let geo_metadata = build_geo_metadata(
    &analysis.geometry_spec.column,
    &geometry_types,
    analysis.full_extent,
    &analysis.spatial_reference,
    analysis.has_z,
    analysis.has_m,
    covering,
    COVERING_BBOX_COLUMN,
  )?;
  let mut metadata = build_geo_key_values(source_metadata, geo_metadata);
  let geodisplay = match analysis.geometry_family {
    GeometryFamily::Point => GeodisplayMetadata::point(DisplayIndexZ::new(
      POINT_Z_CODE_COLUMN,
      POINT_X_COLUMN,
      POINT_Y_COLUMN,
      DEFAULT_COORDINATE_PRECISION,
      analysis.full_extent,
      analysis.spatial_reference.wkid,
      analysis.spatial_reference.wkt.clone(),
      false,
      false,
    )),
    GeometryFamily::NonPoint => GeodisplayMetadata::xz_with_parent(
      DISPLAY_COLUMN,
      DisplayIndexXz::new(
        XZ_CODE_COLUMN,
        "esriPBF",
        analysis.geometry_type.as_str(),
        BOUNDS_COLUMN,
        analysis.full_extent,
        DEFAULT_XZ_MAX_LEVEL,
        analysis.spatial_reference.wkid,
        analysis.spatial_reference.wkt.clone(),
        false,
        false,
        metadata_levels(encodings),
      ),
    ),
  };
  metadata.push(KeyValue {
    key: "geodisplay".to_string(),
    value: Some(serde_json::to_string(&geodisplay)?),
  });
  Ok(metadata)
}

fn fallback_geometry_kind(analysis: &DisplayJobAnalysis) -> GeometryKind {
  match analysis.geometry_type {
    crate::analysis::DisplayGeometryType::Point => GeometryKind::Point,
    crate::analysis::DisplayGeometryType::MultiPoint => GeometryKind::MultiPoint,
    crate::analysis::DisplayGeometryType::Polyline => GeometryKind::LineString,
    crate::analysis::DisplayGeometryType::Polygon => GeometryKind::Polygon,
  }
}

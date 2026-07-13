//! Extends shared GeoParquet metadata with SOP geodisplay metadata.

use anyhow::Result;
use parquet::file::metadata::KeyValue;

use crate::geometry::GeometryKind;
use crate::geoparquet::{GeoMetadataInput, build_geo_key_values, build_geo_metadata};
use crate::optimized::clustering::{DEFAULT_COORDINATE_PRECISION, DEFAULT_XZ_MAX_LEVEL};
use crate::optimized::metadata::{
  GeodisplayMetadata, XzClusteringIndex, XzClusteringIndexInput, ZClusteringIndex,
  ZClusteringIndexInput,
};
use crate::optimized::multiscale::{
  BOUNDS_COLUMN, COVERING_BBOX_COLUMN, GEODISPLAY_COLUMN, POINT_X_COLUMN, POINT_Y_COLUMN,
  POINT_Z_CODE_COLUMN, XZ_CODE_COLUMN, metadata_levels,
};
use crate::optimized::{ClusteringFamily, ResolvedOptimization};

/// Build GeoParquet and geodisplay metadata for optimized output.
pub(crate) fn build_optimized_metadata(
  context: &ResolvedOptimization,
  covering: bool,
) -> Result<Vec<KeyValue>> {
  let source_geometry = context
    .source_metadata
    .geometry
    .as_ref()
    .filter(|geometry| geometry.column == context.geometry.geometry_spec.column);
  let geometry_types = source_geometry
    .filter(|geometry| !geometry.geometry_types.is_empty())
    .map(|geometry| geometry.geometry_types.clone())
    .unwrap_or_else(|| vec![fallback_geometry_kind(context)]);
  let geo_metadata = build_geo_metadata(GeoMetadataInput {
    geometry_column: &context.geometry.geometry_spec.column,
    geometry_types: &geometry_types,
    output_extent: context.target_extent,
    output_spatial_reference: context.reprojection.target_spatial_reference(),
    has_z: context.geometry.has_z,
    has_m: context.geometry.has_m,
    covering,
    covering_column: COVERING_BBOX_COLUMN,
  })?;
  let mut metadata = build_geo_key_values(&context.source_metadata, geo_metadata);
  let geodisplay = match context.geometry.clustering_family {
    ClusteringFamily::Point => {
      GeodisplayMetadata::point(ZClusteringIndex::new(ZClusteringIndexInput {
        code: POINT_Z_CODE_COLUMN.to_string(),
        x_column: POINT_X_COLUMN.to_string(),
        y_column: POINT_Y_COLUMN.to_string(),
        coordinate_precision: DEFAULT_COORDINATE_PRECISION,
        full_extent: context.target_extent,
        wkid: context.reprojection.target_spatial_reference().wkid,
        wkt: context.reprojection.target_spatial_reference().wkt.clone(),
        has_z: false,
        has_m: false,
      }))
    }
    ClusteringFamily::NonPoint => GeodisplayMetadata::xz_with_parent(
      GEODISPLAY_COLUMN,
      XzClusteringIndex::new(XzClusteringIndexInput {
        code: XZ_CODE_COLUMN.to_string(),
        encoding: "esriPBF".to_string(),
        geometry_type: context.geometry.geometry_type.as_str().to_string(),
        bounds: BOUNDS_COLUMN.to_string(),
        full_extent: context.target_extent,
        max_level: DEFAULT_XZ_MAX_LEVEL,
        wkid: context.reprojection.target_spatial_reference().wkid,
        wkt: context.reprojection.target_spatial_reference().wkt.clone(),
        has_z: false,
        has_m: false,
        levels: metadata_levels(&context.encodings),
      }),
    ),
  };
  metadata.push(KeyValue {
    key: "geodisplay".to_string(),
    value: Some(serde_json::to_string(&geodisplay)?),
  });
  Ok(metadata)
}

fn fallback_geometry_kind(context: &ResolvedOptimization) -> GeometryKind {
  match context.geometry.geometry_type {
    crate::optimized::OptimizedGeometryType::Point => GeometryKind::Point,
    crate::optimized::OptimizedGeometryType::MultiPoint => GeometryKind::MultiPoint,
    crate::optimized::OptimizedGeometryType::Polyline => GeometryKind::LineString,
    crate::optimized::OptimizedGeometryType::Polygon => GeometryKind::Polygon,
  }
}

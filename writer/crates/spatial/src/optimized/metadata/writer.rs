//! Extends shared GeoParquet metadata with SOP geodisplay metadata.

use anyhow::Result;
use parquet::file::metadata::KeyValue;

use crate::geometry::GeometryKind;
use crate::geoparquet::{GeoMetadata, GeoMetadataInput};
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
use crate::output::ParquetMetadataSet;

impl ResolvedOptimization {
  /// Serialize GeoParquet and geodisplay metadata for optimized output.
  pub(crate) fn parquet_metadata(&self, covering: bool) -> Result<Vec<KeyValue>> {
    let source_geometry = self
      .source_metadata
      .geometry
      .as_ref()
      .filter(|geometry| geometry.column == self.geometry.geometry_spec.column);
    let geometry_types = source_geometry
      .filter(|geometry| !geometry.geometry_types.is_empty())
      .map(|geometry| geometry.geometry_types.clone())
      .unwrap_or_else(|| vec![self.fallback_geometry_kind()]);
    let geo_metadata = GeoMetadata::new(GeoMetadataInput {
      geometry_column: &self.geometry.geometry_spec.column,
      geometry_types: &geometry_types,
      output_extent: self.target_extent,
      output_spatial_reference: self.reprojection.target_spatial_reference(),
      has_z: self.geometry.has_z,
      has_m: self.geometry.has_m,
      covering,
      covering_column: COVERING_BBOX_COLUMN,
    })?;
    let mut metadata = ParquetMetadataSet::new(self.source_metadata.passthrough_kv.clone());
    metadata.insert(&geo_metadata)?;
    let geodisplay = match self.geometry.clustering_family {
      ClusteringFamily::Point => {
        GeodisplayMetadata::point(ZClusteringIndex::new(ZClusteringIndexInput {
          code: POINT_Z_CODE_COLUMN.to_string(),
          x_column: POINT_X_COLUMN.to_string(),
          y_column: POINT_Y_COLUMN.to_string(),
          coordinate_precision: DEFAULT_COORDINATE_PRECISION,
          full_extent: self.target_extent,
          wkid: self.reprojection.target_spatial_reference().wkid,
          wkt: self.reprojection.target_spatial_reference().wkt.clone(),
          has_z: false,
          has_m: false,
        }))
      }
      ClusteringFamily::NonPoint => GeodisplayMetadata::xz_with_parent(
        GEODISPLAY_COLUMN,
        XzClusteringIndex::new(XzClusteringIndexInput {
          code: XZ_CODE_COLUMN.to_string(),
          encoding: "esriPBF".to_string(),
          geometry_type: self.geometry.geometry_type.as_str().to_string(),
          bounds: BOUNDS_COLUMN.to_string(),
          full_extent: self.target_extent,
          max_level: DEFAULT_XZ_MAX_LEVEL,
          wkid: self.reprojection.target_spatial_reference().wkid,
          wkt: self.reprojection.target_spatial_reference().wkt.clone(),
          has_z: false,
          has_m: false,
          levels: metadata_levels(&self.encodings),
        }),
      ),
    };
    metadata.insert(&geodisplay)?;
    Ok(metadata.into_entries())
  }

  fn fallback_geometry_kind(&self) -> GeometryKind {
    match self.geometry.geometry_type {
      crate::optimized::OptimizedGeometryType::Point => GeometryKind::Point,
      crate::optimized::OptimizedGeometryType::MultiPoint => GeometryKind::MultiPoint,
      crate::optimized::OptimizedGeometryType::Polyline => GeometryKind::LineString,
      crate::optimized::OptimizedGeometryType::Polygon => GeometryKind::Polygon,
    }
  }
}

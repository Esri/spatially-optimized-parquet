//! Extends GeoParquet metadata with spatially optimized output metadata.

use anyhow::Result;
use parquet::file::metadata::KeyValue;

use crate::geometry::GeometryKind;
use crate::geoparquet::{GeoMetadataInput, geo_metadata_entry};
use crate::optimized::clustering::{DEFAULT_COORDINATE_PRECISION, DEFAULT_XZ_MAX_LEVEL};
use crate::optimized::geodisplay_metadata::{
  GeodisplayMetadata, MultiscaleLevelInput, XzClusteringIndex, XzClusteringIndexInput,
  ZClusteringIndex, ZClusteringIndexInput, geodisplay_metadata_entry,
};
use crate::optimized::multiscale::{
  COVERING_BBOX_COLUMN, GEODISPLAY_COLUMN, POINT_M_COLUMN, POINT_X_COLUMN, POINT_Y_COLUMN,
  POINT_Z_CODE_COLUMN, POINT_Z_COLUMN, XZ_CODE_COLUMN,
};
use crate::optimized::{ClusteringFamily, ResolvedOptimization};

/// Serialize GeoParquet and Geodisplay metadata for optimized output.
pub(super) fn parquet_metadata(
  optimization: &ResolvedOptimization,
  covering: bool,
) -> Result<Vec<KeyValue>> {
  optimization.parquet_metadata(covering)
}

impl ResolvedOptimization {
  /// Serialize GeoParquet and geodisplay metadata for optimized output.
  pub(super) fn parquet_metadata(&self, covering: bool) -> Result<Vec<KeyValue>> {
    let source_geometry = self
      .source_metadata()
      .geometry
      .as_ref()
      .filter(|geometry| geometry.column == self.geometry().geometry.column);
    let geometry_types = source_geometry
      .filter(|geometry| !geometry.geometry_types.is_empty())
      .map(|geometry| geometry.geometry_types.clone())
      .unwrap_or_else(|| vec![self.fallback_geometry_kind()]);
    let geo_metadata = GeoMetadataInput {
      geometry_column: &self.geometry().geometry.column,
      geometry_types: &geometry_types,
      output_extent: self.target_extent(),
      output_spatial_reference: self.reprojection().target_spatial_reference(),
      has_z: self.geometry().has_z,
      has_m: self.geometry().has_m,
      covering,
      covering_column: COVERING_BBOX_COLUMN,
    };
    let source_entries = self.source_metadata().passthrough_kv.clone();
    match self.geometry().clustering_family {
      ClusteringFamily::PointGeometry => optimized_point_metadata(
        source_entries,
        geo_metadata,
        GEODISPLAY_COLUMN,
        ZClusteringIndexInput {
          code: POINT_Z_CODE_COLUMN.to_string(),
          x_column: POINT_X_COLUMN.to_string(),
          y_column: POINT_Y_COLUMN.to_string(),
          z_column: self.geometry().has_z.then(|| POINT_Z_COLUMN.to_string()),
          m_column: self.geometry().has_m.then(|| POINT_M_COLUMN.to_string()),
          coordinate_precision: DEFAULT_COORDINATE_PRECISION,
          full_extent: self.target_extent(),
          wkid: self.reprojection().target_spatial_reference().wkid,
          wkt: None,
          has_z: self.geometry().has_z,
          has_m: self.geometry().has_m,
        },
      ),
      ClusteringFamily::ComplexGeometry => optimized_xz_metadata(
        source_entries,
        geo_metadata,
        GEODISPLAY_COLUMN,
        XzClusteringIndexInput {
          code: XZ_CODE_COLUMN.to_string(),
          encoding: self.multiscale_encoding().metadata_identifier().to_string(),
          geometry_type: self.geometry().ty.as_str().to_string(),
          full_extent: self.target_extent(),
          max_level: DEFAULT_XZ_MAX_LEVEL,
          wkid: self.reprojection().target_spatial_reference().wkid,
          wkt: None,
          has_z: self.geometry().has_z,
          has_m: self.geometry().has_m,
          levels: self
            .levels()
            .iter()
            .map(|encoding| MultiscaleLevelInput {
              column: encoding.column.clone(),
              level: encoding.level,
              resolution: encoding.resolution,
              scale: encoding.scale,
              transform_scale: encoding.transform.scale,
              transform_translate: encoding.transform.translate,
            })
            .collect(),
        },
      ),
    }
  }

  fn fallback_geometry_kind(&self) -> GeometryKind {
    match self.geometry().ty {
      crate::geometry::GeometryType::Point => GeometryKind::Point,
      crate::geometry::GeometryType::MultiPoint => GeometryKind::MultiPoint,
      crate::geometry::GeometryType::Polyline => GeometryKind::LineString,
      crate::geometry::GeometryType::Polygon => GeometryKind::Polygon,
    }
  }
}

fn optimized_point_metadata(
  source_entries: Vec<KeyValue>,
  geo_input: GeoMetadataInput<'_>,
  field: &str,
  index_input: ZClusteringIndexInput,
) -> Result<Vec<KeyValue>> {
  optimized_metadata(
    source_entries,
    geo_input,
    GeodisplayMetadata::point(field, ZClusteringIndex::new(index_input)),
  )
}

fn optimized_xz_metadata(
  source_entries: Vec<KeyValue>,
  geo_input: GeoMetadataInput<'_>,
  field: &str,
  index_input: XzClusteringIndexInput,
) -> Result<Vec<KeyValue>> {
  optimized_metadata(
    source_entries,
    geo_input,
    GeodisplayMetadata::xz(field, XzClusteringIndex::new(index_input)),
  )
}

fn optimized_metadata(
  mut source_entries: Vec<KeyValue>,
  geo_input: GeoMetadataInput<'_>,
  geodisplay: GeodisplayMetadata,
) -> Result<Vec<KeyValue>> {
  replace_metadata_entry(&mut source_entries, geo_metadata_entry(geo_input)?);
  replace_metadata_entry(&mut source_entries, geodisplay_metadata_entry(&geodisplay)?);
  Ok(source_entries)
}

fn replace_metadata_entry(entries: &mut Vec<KeyValue>, replacement: KeyValue) {
  entries.retain(|entry| entry.key != replacement.key);
  entries.push(replacement);
}

#[cfg(test)]
#[path = "metadata_tests.rs"]
mod tests;

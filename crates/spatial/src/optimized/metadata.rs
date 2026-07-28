//! Extends GeoParquet metadata with spatially optimized output metadata.

use parquet::file::metadata::KeyValue;

use crate::GeoParquetError;
use crate::geometry::GeometryKind;
use crate::geoparquet::{
  COVERING_BBOX_COLUMN, GeoMetadata, GeoMetadataInput, LodEncoding, LodLevel, LodMetadata,
  LodTransform, OrderingMetadata, XzOrderingMetadata, ZOrderingMetadata,
};
use crate::optimized::geodisplay_metadata::{
  ClusteringIndexXZ, ClusteringIndexXZInput, ClusteringIndexZ, ClusteringIndexZInput, ColumnPath,
  GeodisplayEncoding, GeodisplayMetadata, MultiscaleLevelInput,
};
use crate::optimized::multiscale::{
  GEOKEY_COLUMN, GEOLOD_COLUMN, POINT_M_COLUMN, POINT_X_COLUMN, POINT_Y_COLUMN, POINT_Z_COLUMN,
  SOP_GEOMETRY_COLUMN,
};
use crate::optimized::{ClusteringFamily, OptimizedLayout};
use crate::pipeline::SpatialWriteContext;

impl OptimizedLayout {
  /// Serialize GeoParquet and geodisplay metadata for optimized output.
  pub(crate) fn parquet_metadata(
    &self,
    context: &SpatialWriteContext,
    covering: bool,
  ) -> Result<Vec<KeyValue>, GeoParquetError> {
    let source_geometry = context
      .source()
      .source_metadata
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
      output_extent: context.target_extent(),
      output_spatial_reference: context.reprojection().target_spatial_reference(),
      has_z: self.geometry().has_z,
      has_m: self.geometry().has_m,
      covering,
      covering_column: COVERING_BBOX_COLUMN,
      ordering: self
        .writes_extensions()
        .then(|| self.ordering_metadata(context))
        .flatten(),
      lod: self
        .writes_extensions()
        .then(|| self.lod_metadata(context))
        .flatten(),
    };
    let source_entries = context.source().source_metadata.passthrough_kv.clone();
    match self.geometry().clustering_family {
      ClusteringFamily::PointGeometry => Self::optimized_point_metadata(
        source_entries,
        geo_metadata,
        self.writes_sop().then(|| ClusteringIndexZInput {
          code: ColumnPath::Root(GEOKEY_COLUMN.to_string()),
          x_column: ColumnPath::nested(SOP_GEOMETRY_COLUMN, POINT_X_COLUMN),
          y_column: ColumnPath::nested(SOP_GEOMETRY_COLUMN, POINT_Y_COLUMN),
          z_column: self
            .geometry()
            .has_z
            .then(|| ColumnPath::nested(SOP_GEOMETRY_COLUMN, POINT_Z_COLUMN)),
          m_column: self
            .geometry()
            .has_m
            .then(|| ColumnPath::nested(SOP_GEOMETRY_COLUMN, POINT_M_COLUMN)),
          coordinate_precision: self.cluster_depth(),
          full_extent: context.target_extent(),
          wkid: context.reprojection().target_spatial_reference().wkid,
          wkt: None,
          has_z: self.geometry().has_z,
          has_m: self.geometry().has_m,
        }),
      ),
      ClusteringFamily::ComplexGeometry => Self::optimized_xz_metadata(
        source_entries,
        geo_metadata,
        self.writes_sop().then(|| ClusteringIndexXZInput {
          code: ColumnPath::Root(GEOKEY_COLUMN.to_string()),
          encoding: GeodisplayEncoding::from(self.multiscale_encoding()),
          geometry_type: self.geometry().ty,
          full_extent: context.target_extent(),
          max_level: self.cluster_depth(),
          wkid: context.reprojection().target_spatial_reference().wkid,
          wkt: None,
          has_z: self.geometry().has_z,
          has_m: self.geometry().has_m,
          levels: self
            .levels()
            .iter()
            .map(|encoding| MultiscaleLevelInput {
              column: ColumnPath::nested(GEOLOD_COLUMN, &encoding.column),
              level: encoding.level,
              resolution: encoding.resolution,
              scale: encoding.scale,
              transform_scale: encoding.transform.scale,
              transform_translate: encoding.transform.translate,
            })
            .collect(),
        }),
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

  fn ordering_metadata(&self, context: &SpatialWriteContext) -> Option<OrderingMetadata> {
    let geometry_column = self.geometry().geometry.column.clone();
    let extent = [
      context.target_extent().xmin,
      context.target_extent().ymin,
      context.target_extent().xmax,
      context.target_extent().ymax,
    ];
    Some(match self.geometry().clustering_family {
      ClusteringFamily::PointGeometry => OrderingMetadata::Z(ZOrderingMetadata {
        geometry_column,
        extent,
        bit_width: self.cluster_depth() as u8,
      }),
      ClusteringFamily::ComplexGeometry => OrderingMetadata::Xz(XzOrderingMetadata {
        geometry_column,
        extent,
        max_level: self.cluster_depth() as u8,
      }),
    })
  }

  fn lod_metadata(&self, _context: &SpatialWriteContext) -> Option<LodMetadata> {
    if !self.writes_extension_lod()
      || self.multiscale_encoding() != crate::optimized::MultiscaleEncoding::Pbf
    {
      return None;
    }
    Some(LodMetadata {
      geometry_column: self.geometry().geometry.column.clone(),
      encoding: LodEncoding::Pbf,
      orientation: (self.geometry().ty == crate::geometry::GeometryType::Polygon)
        .then(|| "clockwise".to_string()),
      levels: self
        .levels()
        .iter()
        .map(|level| LodLevel {
          column: [GEOLOD_COLUMN.to_string(), level.column.clone()],
          resolution: level.resolution,
          transform: LodTransform {
            scale: level.transform.scale,
            translate: level.transform.translate,
          },
        })
        .collect(),
    })
  }

  fn optimized_point_metadata(
    source_entries: Vec<KeyValue>,
    geo_input: GeoMetadataInput<'_>,
    index_input: Option<ClusteringIndexZInput>,
  ) -> Result<Vec<KeyValue>, GeoParquetError> {
    Self::optimized_metadata(
      source_entries,
      geo_input,
      index_input.map(|input| GeodisplayMetadata::point(ClusteringIndexZ::new(input))),
    )
  }

  fn optimized_xz_metadata(
    source_entries: Vec<KeyValue>,
    geo_input: GeoMetadataInput<'_>,
    index_input: Option<ClusteringIndexXZInput>,
  ) -> Result<Vec<KeyValue>, GeoParquetError> {
    Self::optimized_metadata(
      source_entries,
      geo_input,
      index_input.map(|input| GeodisplayMetadata::xz(ClusteringIndexXZ::new(input))),
    )
  }

  fn optimized_metadata(
    mut source_entries: Vec<KeyValue>,
    geo_input: GeoMetadataInput<'_>,
    geodisplay: Option<GeodisplayMetadata>,
  ) -> Result<Vec<KeyValue>, GeoParquetError> {
    source_entries.retain(|entry| entry.key != "geo" && entry.key != "geodisplay");
    Self::replace_metadata_entry(&mut source_entries, GeoMetadata::parquet_entry(geo_input)?);
    if let Some(geodisplay) = geodisplay {
      Self::replace_metadata_entry(&mut source_entries, geodisplay.parquet_entry()?);
    }
    Ok(source_entries)
  }

  fn replace_metadata_entry(entries: &mut Vec<KeyValue>, replacement: KeyValue) {
    entries.retain(|entry| entry.key != replacement.key);
    entries.push(replacement);
  }
}

#[cfg(test)]
#[path = "metadata_tests.rs"]
mod tests;

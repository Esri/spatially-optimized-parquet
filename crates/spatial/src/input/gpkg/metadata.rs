// Copyright 2026 Esri
//
// Licensed under the Apache License Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use gdal::Dataset;
use gdal::spatial_ref::SpatialRef;
use gdal::vector::{LayerAccess, geometry_type_to_name};
use gdal_sys::OGRwkbGeometryType;

use crate::geometry::Extent2D;
use crate::geometry::{GeometryEncoding, GeometryType};
use crate::input::{InputError, InputOpenOptions, SourceGeometryMetadata};

const MAX_GEOMETRY_TYPE_SAMPLE_FEATURES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Summarizes one layer for selection error messages.
pub(super) struct GpkgLayerSummary {
  name: String,
  geometry_type: String,
  feature_count: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Captures whether sampled features establish one geometry type or mixed content.
enum SampledGeometryType {
  Concrete {
    ty: GeometryType,
    has_z: bool,
    has_m: bool,
  },
  Mixed,
}

impl GpkgLayerSummary {
  /// Collect user-facing metadata for every vector layer in a dataset.
  pub(super) fn collect(dataset: &Dataset) -> Result<Vec<Self>, InputError> {
    let layer_summaries = dataset
      .layers()
      .map(|mut layer| {
        let feature_count = layer.try_feature_count();
        let declared_geometry_type = layer
          .defn()
          .geom_fields()
          .next()
          .map(|field| Self::geometry_type_hint(field.field_type()));
        let geometry_type = match declared_geometry_type {
          Some(declared_type) if declared_type == "Geometry" => {
            SampledGeometryType::from_layer(&mut layer)
              .map(|sampled_type| sampled_type.hint())
              .unwrap_or(declared_type)
          }
          Some(declared_type) => declared_type,
          None => "None".to_string(),
        };

        Self {
          name: layer.name(),
          geometry_type,
          feature_count,
        }
      })
      .collect::<Vec<_>>();
    if layer_summaries.is_empty() {
      return Err(InputError::Metadata(
        "GeoPackage contains no vector layers".to_string(),
      ));
    }
    Ok(layer_summaries)
  }

  /// Resolve an explicit layer or require one when multiple layers exist.
  pub(super) fn select_name(
    options: &InputOpenOptions,
    layer_summaries: &[Self],
  ) -> Result<String, InputError> {
    let available = Self::format_collection(layer_summaries);
    if let Some(requested) = options.layer() {
      if layer_summaries.iter().any(|layer| layer.name == requested) {
        return Ok(requested.to_string());
      }
      return Err(InputError::Metadata(format!(
        "GeoPackage layer {:?} was not found in {}\nAvailable layers:\n{}",
        requested,
        options.location(),
        available
      )));
    }

    if layer_summaries.len() == 1 {
      return Ok(layer_summaries[0].name.clone());
    }

    Err(InputError::Metadata(format!(
      "GeoPackage {} contains multiple layers; pass --layer <NAME>\nAvailable layers:\n{}",
      options.location(),
      available
    )))
  }

  fn format_collection(layer_summaries: &[Self]) -> String {
    layer_summaries
      .iter()
      .map(|layer| {
        let mut hints = Vec::new();
        hints.push(format!("geometry: {}", layer.geometry_type));
        if let Some(feature_count) = layer.feature_count {
          hints.push(format!(
            "features: {}",
            Self::format_feature_count(feature_count)
          ));
        }
        format!("- {} ({})", layer.name, hints.join(", "))
      })
      .collect::<Vec<_>>()
      .join("\n")
  }

  fn geometry_type_hint(geometry_type: OGRwkbGeometryType::Type) -> String {
    let (ty, has_z, has_m) = SourceGeometryMetadata::from_gpkg_geometry_type(geometry_type);
    if let Some(ty) = ty {
      return Self::type_hint(ty, has_z, has_m);
    }

    let raw_name = geometry_type_to_name(geometry_type);
    match raw_name.as_str() {
      "" => "Unknown".to_string(),
      "Unknown (any)" | "Unknown" => "Geometry".to_string(),
      other => other.to_string(),
    }
  }

  fn type_hint(ty: GeometryType, has_z: bool, has_m: bool) -> String {
    let base = Self::type_label(ty).unwrap_or("Geometry");
    let suffix = match (has_z, has_m) {
      (false, false) => "",
      (true, false) => " Z",
      (false, true) => " M",
      (true, true) => " ZM",
    };
    format!("{base}{suffix}")
  }

  fn type_label(ty: GeometryType) -> Option<&'static str> {
    match ty {
      GeometryType::Point => Some("Point"),
      GeometryType::LineString => Some("LineString"),
      GeometryType::MultiPoint => Some("MultiPoint"),
      GeometryType::MultiLineString => Some("MultiLineString"),
      GeometryType::Polygon => Some("Polygon"),
      GeometryType::MultiPolygon => Some("MultiPolygon"),
      GeometryType::GeometryCollection => Some("GeometryCollection"),
      GeometryType::Unknown => None,
    }
  }

  fn format_feature_count(feature_count: u64) -> String {
    let digits = feature_count.to_string();
    let mut reversed = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().rev().enumerate() {
      if index > 0 && index % 3 == 0 {
        reversed.push(',');
      }
      reversed.push(ch);
    }
    reversed.chars().rev().collect()
  }
}

impl SampledGeometryType {
  /// Inspect a bounded feature sample when GDAL reports a generic geometry type.
  fn from_layer(layer: &mut impl LayerAccess) -> Option<Self> {
    let mut sampled_geometry: Option<(GeometryType, bool, bool)> = None;

    for feature in layer.features().take(MAX_GEOMETRY_TYPE_SAMPLE_FEATURES) {
      let Ok(geometry) = feature.geometry_by_index(0) else {
        continue;
      };
      if geometry.is_empty() {
        continue;
      }

      let (ty, has_z, has_m) =
        SourceGeometryMetadata::from_gpkg_geometry_type(geometry.geometry_type());
      let ty = ty?;
      match sampled_geometry {
        Some((existing_ty, existing_has_z, existing_has_m))
          if existing_ty != ty || existing_has_z != has_z || existing_has_m != has_m =>
        {
          return Some(Self::Mixed);
        }
        Some(_) => {}
        None => sampled_geometry = Some((ty, has_z, has_m)),
      }
    }

    sampled_geometry.map(|(ty, has_z, has_m)| Self::Concrete { ty, has_z, has_m })
  }

  fn hint(self) -> String {
    match self {
      Self::Concrete { ty, has_z, has_m } => GpkgLayerSummary::type_hint(ty, has_z, has_m),
      Self::Mixed => "Mixed".to_string(),
    }
  }
}

impl SourceGeometryMetadata {
  /// Construct normalized source geometry metadata from a GDAL layer.
  pub(super) fn from_gpkg_layer(
    layer: &mut impl LayerAccess,
    layer_name: &str,
  ) -> Result<SourceGeometryMetadata, InputError> {
    let (column_name, geometry_type, field_spatial_ref) = {
      let geom_field = layer.defn().geom_fields().next().ok_or_else(|| {
        InputError::Metadata(format!(
          "GeoPackage layer {layer_name} has no geometry column"
        ))
      })?;
      (
        geom_field.name(),
        geom_field.field_type(),
        geom_field.spatial_ref().ok(),
      )
    };
    let (mut ty, mut has_z, mut has_m) = Self::from_gpkg_geometry_type(geometry_type);
    if ty.is_none()
      && let Some(SampledGeometryType::Concrete {
        ty: sampled_ty,
        has_z: sampled_has_z,
        has_m: sampled_has_m,
      }) = SampledGeometryType::from_layer(layer)
    {
      ty = Some(sampled_ty);
      has_z = sampled_has_z;
      has_m = sampled_has_m;
    }
    let projjson = field_spatial_ref
      .or_else(|| layer.spatial_ref())
      .and_then(|spatial_ref| Self::spatial_ref_to_projjson(&spatial_ref));
    let bbox = layer
      .try_get_extent()
      .map_err(|source| InputError::GeoPackage {
        operation: "read GeoPackage layer extent",
        source,
      })?
      .map(|extent| Extent2D {
        xmin: extent.MinX,
        ymin: extent.MinY,
        xmax: extent.MaxX,
        ymax: extent.MaxY,
      });

    Ok(SourceGeometryMetadata {
      column: column_name,
      encoding: GeometryEncoding::Wkb,
      geometry_types: ty.into_iter().collect(),
      bbox,
      covering: None,
      projjson,
      has_z,
      has_m,
    })
  }

  fn spatial_ref_to_projjson(spatial_ref: &SpatialRef) -> Option<serde_json::Value> {
    spatial_ref
      .to_projjson()
      .ok()
      .and_then(|projjson| serde_json::from_str(&projjson).ok())
  }

  #[allow(non_upper_case_globals)]
  fn from_gpkg_geometry_type(
    geometry_type: OGRwkbGeometryType::Type,
  ) -> (Option<GeometryType>, bool, bool) {
    use OGRwkbGeometryType::*;

    match geometry_type {
      wkbPoint => (Some(GeometryType::Point), false, false),
      wkbLineString => (Some(GeometryType::LineString), false, false),
      wkbPolygon => (Some(GeometryType::Polygon), false, false),
      wkbMultiPoint => (Some(GeometryType::MultiPoint), false, false),
      wkbMultiLineString => (Some(GeometryType::MultiLineString), false, false),
      wkbMultiPolygon => (Some(GeometryType::MultiPolygon), false, false),
      wkbGeometryCollection => (Some(GeometryType::GeometryCollection), false, false),
      wkbPoint25D => (Some(GeometryType::Point), true, false),
      wkbLineString25D => (Some(GeometryType::LineString), true, false),
      wkbPolygon25D => (Some(GeometryType::Polygon), true, false),
      wkbMultiPoint25D => (Some(GeometryType::MultiPoint), true, false),
      wkbMultiLineString25D => (Some(GeometryType::MultiLineString), true, false),
      wkbMultiPolygon25D => (Some(GeometryType::MultiPolygon), true, false),
      wkbGeometryCollection25D => (Some(GeometryType::GeometryCollection), true, false),
      wkbPointM => (Some(GeometryType::Point), false, true),
      wkbLineStringM => (Some(GeometryType::LineString), false, true),
      wkbPolygonM => (Some(GeometryType::Polygon), false, true),
      wkbMultiPointM => (Some(GeometryType::MultiPoint), false, true),
      wkbMultiLineStringM => (Some(GeometryType::MultiLineString), false, true),
      wkbMultiPolygonM => (Some(GeometryType::MultiPolygon), false, true),
      wkbGeometryCollectionM => (Some(GeometryType::GeometryCollection), false, true),
      wkbPointZM => (Some(GeometryType::Point), true, true),
      wkbLineStringZM => (Some(GeometryType::LineString), true, true),
      wkbPolygonZM => (Some(GeometryType::Polygon), true, true),
      wkbMultiPointZM => (Some(GeometryType::MultiPoint), true, true),
      wkbMultiLineStringZM => (Some(GeometryType::MultiLineString), true, true),
      wkbMultiPolygonZM => (Some(GeometryType::MultiPolygon), true, true),
      wkbGeometryCollectionZM => (Some(GeometryType::GeometryCollection), true, true),
      _ => (None, false, false),
    }
  }
}

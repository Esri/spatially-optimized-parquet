use anyhow::{Result, anyhow, bail};
use gdal::Dataset;
use gdal::spatial_ref::SpatialRef;
use gdal::vector::{LayerAccess, geometry_type_to_name};
use gdal_sys::OGRwkbGeometryType;

use crate::geometry::Extent2D;
use crate::geometry::{GeometryEncoding, GeometryKind};
use crate::geoparquet::metadata::source::SourceGeometryMetadata;
use crate::input::InputOpenOptions;

const MAX_GEOMETRY_TYPE_SAMPLE_FEATURES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Summarizes one layer for selection errors and diagnostics.
pub(super) struct GpkgLayerSummary {
  name: String,
  geometry_type: String,
  feature_count: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Captures whether sampled features establish one geometry type or mixed content.
enum SampledGeometryType {
  Concrete {
    geometry_kind: GeometryKind,
    has_z: bool,
    has_m: bool,
  },
  Mixed,
}

/// Collect user-facing metadata for every vector layer in a dataset.
pub(super) fn collect_layer_summaries(dataset: &Dataset) -> Result<Vec<GpkgLayerSummary>> {
  let layer_summaries = dataset
    .layers()
    .map(|mut layer| {
      let feature_count = layer.try_feature_count();
      let declared_geometry_type = layer
        .defn()
        .geom_fields()
        .next()
        .map(|field| geometry_type_hint(field.field_type()));
      let geometry_type = match declared_geometry_type {
        Some(declared_type) if declared_type == "Geometry" => {
          sample_geometry_type_hint(&mut layer).unwrap_or(declared_type)
        }
        Some(declared_type) => declared_type,
        None => "None".to_string(),
      };

      GpkgLayerSummary {
        name: layer.name(),
        geometry_type,
        feature_count,
      }
    })
    .collect::<Vec<_>>();
  if layer_summaries.is_empty() {
    bail!("GeoPackage contains no vector layers");
  }
  Ok(layer_summaries)
}

/// Resolve an explicit layer or require one when multiple layers exist.
pub(super) fn select_layer_name(
  options: &InputOpenOptions,
  layer_summaries: &[GpkgLayerSummary],
) -> Result<String> {
  let available = format_layer_summaries(layer_summaries);
  if let Some(requested) = &options.layer {
    if layer_summaries.iter().any(|layer| &layer.name == requested) {
      return Ok(requested.clone());
    }
    bail!(
      "GeoPackage layer {:?} was not found in {}\nAvailable layers:\n{}",
      requested,
      options.location,
      available
    );
  }

  if layer_summaries.len() == 1 {
    return Ok(layer_summaries[0].name.clone());
  }

  bail!(
    "GeoPackage {} contains multiple layers; pass --layer <NAME>\nAvailable layers:\n{}",
    options.location,
    available
  )
}

fn format_layer_summaries(layer_summaries: &[GpkgLayerSummary]) -> String {
  layer_summaries
    .iter()
    .map(|layer| {
      let mut hints = Vec::new();
      hints.push(format!("geometry: {}", layer.geometry_type));
      if let Some(feature_count) = layer.feature_count {
        hints.push(format!("features: {}", format_feature_count(feature_count)));
      }
      format!("- {} ({})", layer.name, hints.join(", "))
    })
    .collect::<Vec<_>>()
    .join("\n")
}

fn sample_geometry_type_hint(layer: &mut impl LayerAccess) -> Option<String> {
  match sample_geometry_type(layer) {
    Some(SampledGeometryType::Concrete {
      geometry_kind,
      has_z,
      has_m,
    }) => Some(geometry_kind_hint(geometry_kind, has_z, has_m)),
    Some(SampledGeometryType::Mixed) => Some("Mixed".to_string()),
    None => None,
  }
}

/// Inspect a bounded feature sample when GDAL reports a generic geometry type.
fn sample_geometry_type(layer: &mut impl LayerAccess) -> Option<SampledGeometryType> {
  let mut sampled_geometry: Option<(GeometryKind, bool, bool)> = None;

  for feature in layer.features().take(MAX_GEOMETRY_TYPE_SAMPLE_FEATURES) {
    let Ok(geometry) = feature.geometry_by_index(0) else {
      continue;
    };
    if geometry.is_empty() {
      continue;
    }

    let (geometry_kind, has_z, has_m) = map_geometry_type(geometry.geometry_type());
    let geometry_kind = geometry_kind?;
    match sampled_geometry {
      Some((existing_kind, existing_has_z, existing_has_m))
        if existing_kind != geometry_kind || existing_has_z != has_z || existing_has_m != has_m =>
      {
        return Some(SampledGeometryType::Mixed);
      }
      Some(_) => {}
      None => sampled_geometry = Some((geometry_kind, has_z, has_m)),
    }
  }

  sampled_geometry.map(
    |(geometry_kind, has_z, has_m)| SampledGeometryType::Concrete {
      geometry_kind,
      has_z,
      has_m,
    },
  )
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

/// Normalize geometry type, dimensions, extent, and CRS from a GDAL layer.
pub(super) fn build_geometry_metadata(
  layer: &mut impl LayerAccess,
  layer_name: &str,
) -> Result<SourceGeometryMetadata> {
  let (column_name, geometry_type, field_spatial_ref) = {
    let geom_field = layer
      .defn()
      .geom_fields()
      .next()
      .ok_or_else(|| anyhow!("GeoPackage layer {layer_name} has no geometry column"))?;
    (
      geom_field.name(),
      geom_field.field_type(),
      geom_field.spatial_ref().ok(),
    )
  };
  let (mut geometry_kind, mut has_z, mut has_m) = map_geometry_type(geometry_type);
  if geometry_kind.is_none()
    && let Some(SampledGeometryType::Concrete {
      geometry_kind: sampled_kind,
      has_z: sampled_has_z,
      has_m: sampled_has_m,
    }) = sample_geometry_type(layer)
  {
    geometry_kind = Some(sampled_kind);
    has_z = sampled_has_z;
    has_m = sampled_has_m;
  }
  let projjson = field_spatial_ref
    .or_else(|| layer.spatial_ref())
    .and_then(|spatial_ref| spatial_ref_to_projjson(&spatial_ref));
  let bbox = layer.try_get_extent()?.map(|extent| Extent2D {
    xmin: extent.MinX,
    ymin: extent.MinY,
    xmax: extent.MaxX,
    ymax: extent.MaxY,
  });

  Ok(SourceGeometryMetadata {
    column: column_name,
    encoding: GeometryEncoding::Wkb,
    geometry_types: geometry_kind.into_iter().collect(),
    bbox,
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

fn geometry_type_hint(geometry_type: OGRwkbGeometryType::Type) -> String {
  let (geometry_kind, has_z, has_m) = map_geometry_type(geometry_type);
  if let Some(geometry_kind) = geometry_kind {
    return geometry_kind_hint(geometry_kind, has_z, has_m);
  }

  let raw_name = geometry_type_to_name(geometry_type);
  match raw_name.as_str() {
    "" => "Unknown".to_string(),
    "Unknown (any)" | "Unknown" => "Geometry".to_string(),
    other => other.to_string(),
  }
}

fn geometry_kind_hint(geometry_kind: GeometryKind, has_z: bool, has_m: bool) -> String {
  let base = geometry_kind_label(geometry_kind).unwrap_or("Geometry");
  let suffix = match (has_z, has_m) {
    (false, false) => "",
    (true, false) => " Z",
    (false, true) => " M",
    (true, true) => " ZM",
  };
  format!("{base}{suffix}")
}

fn geometry_kind_label(geometry_kind: GeometryKind) -> Option<&'static str> {
  match geometry_kind {
    GeometryKind::Point => Some("Point"),
    GeometryKind::LineString => Some("LineString"),
    GeometryKind::MultiPoint => Some("MultiPoint"),
    GeometryKind::MultiLineString => Some("MultiLineString"),
    GeometryKind::Polygon => Some("Polygon"),
    GeometryKind::MultiPolygon => Some("MultiPolygon"),
    GeometryKind::GeometryCollection => Some("GeometryCollection"),
    GeometryKind::Unknown => None,
  }
}

#[allow(non_upper_case_globals)]
fn map_geometry_type(
  geometry_type: OGRwkbGeometryType::Type,
) -> (Option<GeometryKind>, bool, bool) {
  use OGRwkbGeometryType::*;

  match geometry_type {
    wkbPoint => (Some(GeometryKind::Point), false, false),
    wkbLineString => (Some(GeometryKind::LineString), false, false),
    wkbPolygon => (Some(GeometryKind::Polygon), false, false),
    wkbMultiPoint => (Some(GeometryKind::MultiPoint), false, false),
    wkbMultiLineString => (Some(GeometryKind::MultiLineString), false, false),
    wkbMultiPolygon => (Some(GeometryKind::MultiPolygon), false, false),
    wkbGeometryCollection => (Some(GeometryKind::GeometryCollection), false, false),
    wkbPoint25D => (Some(GeometryKind::Point), true, false),
    wkbLineString25D => (Some(GeometryKind::LineString), true, false),
    wkbPolygon25D => (Some(GeometryKind::Polygon), true, false),
    wkbMultiPoint25D => (Some(GeometryKind::MultiPoint), true, false),
    wkbMultiLineString25D => (Some(GeometryKind::MultiLineString), true, false),
    wkbMultiPolygon25D => (Some(GeometryKind::MultiPolygon), true, false),
    wkbGeometryCollection25D => (Some(GeometryKind::GeometryCollection), true, false),
    wkbPointM => (Some(GeometryKind::Point), false, true),
    wkbLineStringM => (Some(GeometryKind::LineString), false, true),
    wkbPolygonM => (Some(GeometryKind::Polygon), false, true),
    wkbMultiPointM => (Some(GeometryKind::MultiPoint), false, true),
    wkbMultiLineStringM => (Some(GeometryKind::MultiLineString), false, true),
    wkbMultiPolygonM => (Some(GeometryKind::MultiPolygon), false, true),
    wkbGeometryCollectionM => (Some(GeometryKind::GeometryCollection), false, true),
    wkbPointZM => (Some(GeometryKind::Point), true, true),
    wkbLineStringZM => (Some(GeometryKind::LineString), true, true),
    wkbPolygonZM => (Some(GeometryKind::Polygon), true, true),
    wkbMultiPointZM => (Some(GeometryKind::MultiPoint), true, true),
    wkbMultiLineStringZM => (Some(GeometryKind::MultiLineString), true, true),
    wkbMultiPolygonZM => (Some(GeometryKind::MultiPolygon), true, true),
    wkbGeometryCollectionZM => (Some(GeometryKind::GeometryCollection), true, true),
    _ => (None, false, false),
  }
}

#[cfg(test)]
mod tests {
  use super::format_feature_count;

  #[test]
  fn feature_count_formatting_uses_commas() {
    assert_eq!(format_feature_count(0), "0");
    assert_eq!(format_feature_count(12), "12");
    assert_eq!(format_feature_count(1_234), "1,234");
    assert_eq!(format_feature_count(26_348_056), "26,348,056");
  }
}

use std::collections::HashMap;

use ::parquet::file::metadata::KeyValue;
use serde_json::{Value, json};

use crate::geometry::{Extent2D, GeometryKind};
use crate::output::SpatialReferenceInfo;

use super::{
  GeoMetadataInput, MultiscaleLevelInput, XzClusteringIndexInput, ZClusteringIndexInput,
  geoparquet_metadata, optimized_point_metadata, optimized_xz_metadata,
};

fn spatial_reference() -> SpatialReferenceInfo {
  SpatialReferenceInfo {
    wkid: Some(4326),
    wkt: Some("EPSG:4326 WKT".to_string()),
    projjson: Some(json!({
      "type": "GeographicCRS",
      "name": "WGS 84",
      "id": {
        "authority": "EPSG",
        "code": 4326
      }
    })),
  }
}

fn geo_input<'a>(
  geometry_types: &'a [GeometryKind],
  spatial_reference: &'a SpatialReferenceInfo,
  covering: bool,
) -> GeoMetadataInput<'a> {
  GeoMetadataInput {
    geometry_column: "geometry",
    geometry_types,
    output_extent: Extent2D {
      xmin: -180.0,
      ymin: -90.0,
      xmax: 180.0,
      ymax: 90.0,
    },
    output_spatial_reference: spatial_reference,
    has_z: true,
    has_m: true,
    covering,
    covering_column: "bbox",
  }
}

fn metadata_values(entries: Vec<KeyValue>) -> HashMap<String, Value> {
  entries
    .into_iter()
    .filter_map(|entry| {
      entry.value.map(|value| {
        (
          entry.key,
          serde_json::from_str(&value).unwrap_or(Value::String(value)),
        )
      })
    })
    .collect()
}

#[test]
fn geo_metadata_serializes_crs_extent_wkb_and_covering() {
  let spatial_reference = spatial_reference();
  let geometry_types = [GeometryKind::Point, GeometryKind::MultiPolygon];
  let values = metadata_values(
    geoparquet_metadata(
      Vec::new(),
      geo_input(&geometry_types, &spatial_reference, true),
    )
    .unwrap(),
  );

  assert_eq!(
    values["geo"],
    json!({
      "version": "1.1.0",
      "primary_column": "geometry",
      "columns": {
        "geometry": {
          "encoding": "WKB",
          "geometry_types": ["Point ZM", "MultiPolygon ZM"],
          "bbox": [-180.0, -90.0, 180.0, 90.0],
          "crs": {
            "type": "GeographicCRS",
            "name": "WGS 84",
            "id": {
              "authority": "EPSG",
              "code": 4326
            }
          },
          "covering": {
            "bbox": {
              "xmin": ["bbox", "xmin"],
              "ymin": ["bbox", "ymin"],
              "xmax": ["bbox", "xmax"],
              "ymax": ["bbox", "ymax"]
            }
          }
        }
      }
    })
  );
}

#[test]
fn geo_metadata_omits_covering_and_replaces_reserved_source_entry() {
  let spatial_reference = spatial_reference();
  let geometry_types = [GeometryKind::Point];
  let values = metadata_values(
    geoparquet_metadata(
      vec![
        KeyValue::new("geo".to_string(), Some("\"stale\"".to_string())),
        KeyValue::new("source".to_string(), Some("\"census\"".to_string())),
      ],
      geo_input(&geometry_types, &spatial_reference, false),
    )
    .unwrap(),
  );

  assert_eq!(values["source"], json!("census"));
  assert!(
    values["geo"]["columns"]["geometry"]
      .get("covering")
      .is_none()
  );
}

#[test]
fn point_geodisplay_metadata_serializes_z_clustering() {
  let spatial_reference = spatial_reference();
  let geometry_types = [GeometryKind::Point];
  let values = metadata_values(
    optimized_point_metadata(
      Vec::new(),
      geo_input(&geometry_types, &spatial_reference, false),
      "geodisplay",
      ZClusteringIndexInput {
        code: "zCode".to_string(),
        x_column: "x".to_string(),
        y_column: "y".to_string(),
        coordinate_precision: 20,
        full_extent: Extent2D {
          xmin: -180.0,
          ymin: -90.0,
          xmax: 180.0,
          ymax: 90.0,
        },
        wkid: Some(4326),
        wkt: None,
        has_z: false,
        has_m: false,
      },
    )
    .unwrap(),
  );

  assert_eq!(
    values["geodisplay"],
    json!({
      "parentColumn": "geodisplay",
      "index": {
        "type": "z",
        "version": "0.1",
        "code": "zCode",
        "wkid": 4326,
        "xColumn": "x",
        "yColumn": "y",
        "coordinatePrecision": 20,
        "fullExtent": {
          "xmin": -180.0,
          "ymin": -90.0,
          "xmax": 180.0,
          "ymax": 90.0
        },
        "geometryType": "point",
        "hasZ": false,
        "hasM": false
      }
    })
  );
}

#[test]
fn xz_geodisplay_metadata_serializes_multiscale_clustering() {
  let spatial_reference = spatial_reference();
  let geometry_types = [GeometryKind::Polygon];
  let values = metadata_values(
    optimized_xz_metadata(
      Vec::new(),
      geo_input(&geometry_types, &spatial_reference, false),
      "geodisplay",
      XzClusteringIndexInput {
        code: "xzCode".to_string(),
        encoding: "esriPBF".to_string(),
        geometry_type: "polygon".to_string(),
        full_extent: Extent2D {
          xmin: -10.0,
          ymin: -5.0,
          xmax: 10.0,
          ymax: 5.0,
        },
        max_level: 20,
        wkid: Some(4326),
        wkt: None,
        has_z: false,
        has_m: false,
        levels: vec![MultiscaleLevelInput {
          column: "level_0".to_string(),
          level: 0,
          resolution: 0.703125,
          scale: 295_829_355.4545656,
          transform_scale: [0.703125, 0.703125, 1.0, 1.0],
          transform_translate: [0.0, 0.0, 0.0, 0.0],
        }],
      },
    )
    .unwrap(),
  );

  assert_eq!(
    values["geodisplay"],
    json!({
      "parentColumn": "geodisplay",
      "index": {
        "type": "xz",
        "version": "0.1",
        "code": "xzCode",
        "wkid": 4326,
        "encoding": "esriPBF",
        "geometryType": "polygon",
        "fullExtent": {
          "xmin": -10.0,
          "ymin": -5.0,
          "xmax": 10.0,
          "ymax": 5.0
        },
        "maxLevel": 20,
        "hasZ": false,
        "hasM": false,
        "levels": [{
          "column": "level_0",
          "level": 0,
          "resolution": 0.703125,
          "scale": 295829355.4545656,
          "transform": {
            "scale": [0.703125, 0.703125, 1.0, 1.0],
            "translate": [0.0, 0.0, 0.0, 0.0]
          }
        }]
      }
    })
  );
}

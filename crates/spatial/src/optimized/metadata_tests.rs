use std::collections::HashMap;

use ::parquet::file::metadata::KeyValue;
use serde_json::{Value, json};

use crate::geometry::{Extent2D, GeometryKind, GeometryType};
use crate::geoparquet::SpatialReference;
use crate::geoparquet::{
  GeoMetadata, GeoMetadataInput, LodEncoding, LodLevel, LodMetadata, LodTransform,
  OrderingMetadata, XzOrderingMetadata,
};

use super::{
  ClusteringIndexXZInput, ClusteringIndexZInput, ColumnPath, GeodisplayEncoding,
  GeodisplayMetadata, MultiscaleLevelInput, OptimizedLayout,
};

fn spatial_reference() -> SpatialReference {
  SpatialReference {
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
  spatial_reference: &'a SpatialReference,
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
    ordering: None,
    lod: None,
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

fn point_index_input() -> ClusteringIndexZInput {
  ClusteringIndexZInput {
    code: ColumnPath::nested("geodisplay", "zCode"),
    x_column: ColumnPath::nested("geodisplay", "x"),
    y_column: ColumnPath::nested("geodisplay", "y"),
    z_column: None,
    m_column: None,
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
  }
}

#[test]
fn geo_metadata_serializes_crs_extent_wkb_and_covering() {
  let spatial_reference = spatial_reference();
  let geometry_types = [GeometryKind::Point, GeometryKind::MultiPolygon];
  let values = metadata_values(
    GeoMetadata::parquet_entries(
      Vec::new(),
      geo_input(&geometry_types, &spatial_reference, true),
    )
    .unwrap(),
  );

  assert_eq!(
    values["geo"],
    json!({
      "version": "2.0.0",
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
fn geo_metadata_serializes_ordering_and_lod_extensions() {
  let spatial_reference = spatial_reference();
  let geometry_types = [GeometryKind::Polygon];
  let mut input = geo_input(&geometry_types, &spatial_reference, false);
  input.ordering = Some(OrderingMetadata::Xz(XzOrderingMetadata {
    geometry_column: "geometry".to_string(),
    extent: [-180.0, -90.0, 180.0, 90.0],
    max_level: 20,
  }));
  input.lod = Some(LodMetadata {
    geometry_column: "geometry".to_string(),
    encoding: LodEncoding::Pbf,
    orientation: Some("clockwise".to_string()),
    levels: vec![LodLevel {
      column: ["geolod".to_string(), "level_0".to_string()],
      resolution: 1.0,
      transform: LodTransform {
        scale: [1.0; 4],
        translate: [0.0; 4],
      },
    }],
  });

  let values = metadata_values(GeoMetadata::parquet_entries(Vec::new(), input).unwrap());

  assert_eq!(values["geo"]["ordering"]["type"], "xz");
  assert_eq!(values["geo"]["ordering"]["geometry_column"], "geometry");
  assert_eq!(values["geo"]["lod"]["encoding"], "pbf");
  assert_eq!(values["geo"]["lod"]["levels"][0]["resolution"], 1.0);
  assert!(values["geo"]["lod"]["levels"][0].get("scale").is_none());
  assert_eq!(
    values["geo"]["lod"]["levels"][0]["column"],
    json!(["geolod", "level_0"])
  );
}

#[test]
fn geo_metadata_omits_covering_and_replaces_reserved_source_entry() {
  let spatial_reference = spatial_reference();
  let geometry_types = [GeometryKind::Point];
  let values = metadata_values(
    GeoMetadata::parquet_entries(
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
fn point_geometry_geodisplay_metadata_serializes_z_clustering() {
  let spatial_reference = spatial_reference();
  let geometry_types = [GeometryKind::Point];
  let values = metadata_values(
    OptimizedLayout::optimized_point_metadata(
      Vec::new(),
      geo_input(&geometry_types, &spatial_reference, false),
      Some(point_index_input()),
    )
    .unwrap(),
  );

  assert_eq!(
    values["geodisplay"],
    json!({
      "type": "z",
      "version": "0.1",
      "writer": {
        "name": "sop",
        "version": env!("CARGO_PKG_VERSION")
      },
      "code": ["geodisplay", "zCode"],
      "wkid": 4326,
      "xColumn": ["geodisplay", "x"],
      "yColumn": ["geodisplay", "y"],
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
    })
  );
  assert!(matches!(
    serde_json::from_value::<GeodisplayMetadata>(values["geodisplay"].clone()).unwrap(),
    GeodisplayMetadata::Z { .. }
  ));
}

#[test]
fn optimized_metadata_selects_sop_and_extensions_independently() {
  let spatial_reference = spatial_reference();
  let geometry_types = [GeometryKind::Point];

  for (write_sop, write_extensions) in [(false, false), (true, false), (false, true), (true, true)]
  {
    let mut input = geo_input(&geometry_types, &spatial_reference, false);
    input.ordering = write_extensions.then(|| {
      OrderingMetadata::Z(crate::geoparquet::ZOrderingMetadata {
        geometry_column: "geometry".to_string(),
        extent: [-180.0, -90.0, 180.0, 90.0],
        bit_width: 32,
      })
    });
    let values = metadata_values(
      OptimizedLayout::optimized_point_metadata(
        vec![
          KeyValue::new("geo".to_string(), Some("\"stale\"".to_string())),
          KeyValue::new("geodisplay".to_string(), Some("\"stale\"".to_string())),
          KeyValue::new("source".to_string(), Some("\"census\"".to_string())),
        ],
        input,
        write_sop.then(point_index_input),
      )
      .unwrap(),
    );

    assert_eq!(values.contains_key("geodisplay"), write_sop);
    assert_eq!(values["geo"].get("ordering").is_some(), write_extensions);
    assert_eq!(values["source"], json!("census"));
  }
}

#[test]
fn xz_geodisplay_metadata_serializes_multiscale_clustering() {
  let spatial_reference = spatial_reference();
  let geometry_types = [GeometryKind::Polygon];
  let values = metadata_values(
    OptimizedLayout::optimized_xz_metadata(
      Vec::new(),
      geo_input(&geometry_types, &spatial_reference, false),
      Some(ClusteringIndexXZInput {
        code: ColumnPath::nested("geodisplay", "xzCode"),
        encoding: GeodisplayEncoding::EsriPbf,
        geometry_type: GeometryType::Polygon,
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
          column: ColumnPath::nested("geodisplay", "level_0"),
          level: 0,
          resolution: 0.703125,
          scale: 295_829_355.4545656,
          transform_scale: [0.703125, 0.703125, 1.0, 1.0],
          transform_translate: [0.0, 0.0, 0.0, 0.0],
        }],
      }),
    )
    .unwrap(),
  );

  assert_eq!(
    values["geodisplay"],
    json!({
      "type": "xz",
      "version": "0.1",
      "writer": {
        "name": "sop",
        "version": env!("CARGO_PKG_VERSION")
      },
      "code": ["geodisplay", "xzCode"],
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
        "column": ["geodisplay", "level_0"],
        "level": 0,
        "resolution": 0.703125,
        "scale": 295829355.4545656,
        "transform": {
          "scale": [0.703125, 0.703125, 1.0, 1.0],
          "translate": [0.0, 0.0, 0.0, 0.0]
        }
      }]
    })
  );
  match serde_json::from_value::<GeodisplayMetadata>(values["geodisplay"].clone()).unwrap() {
    GeodisplayMetadata::Xz { index } => {
      assert_eq!(index.encoding, GeodisplayEncoding::EsriPbf);
      assert_eq!(index.geometry_type, GeometryType::Polygon);
    }
    GeodisplayMetadata::Z { .. } => panic!("XZ metadata decoded as Z metadata"),
  }
}

#[test]
fn geodisplay_metadata_serializes_closed_vocabulary_values() {
  for (encoding, serialized) in [
    (GeodisplayEncoding::EsriPbf, "esriPBF"),
    (GeodisplayEncoding::QuantizedNative, "quantizedNative"),
  ] {
    assert_eq!(serde_json::to_value(encoding).unwrap(), json!(serialized));
    assert_eq!(
      serde_json::from_value::<GeodisplayEncoding>(json!(serialized)).unwrap(),
      encoding
    );
  }

  for (geometry_type, serialized) in [
    (GeometryType::Point, "point"),
    (GeometryType::MultiPoint, "multipoint"),
    (GeometryType::Polyline, "polyline"),
    (GeometryType::Polygon, "polygon"),
  ] {
    assert_eq!(
      serde_json::to_value(geometry_type).unwrap(),
      json!(serialized)
    );
    assert_eq!(
      serde_json::from_value::<GeometryType>(json!(serialized)).unwrap(),
      geometry_type
    );
  }
}

#[test]
fn geodisplay_metadata_rejects_unsupported_closed_vocabulary_values() {
  let metadata = json!({
    "type": "xz",
    "version": "0.1",
    "code": "xzCode",
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
    "levels": []
  });

  let mut unsupported_index = metadata.clone();
  unsupported_index["type"] = json!("future");
  assert!(serde_json::from_value::<GeodisplayMetadata>(unsupported_index).is_err());

  let mut unsupported_encoding = metadata.clone();
  unsupported_encoding["encoding"] = json!("futureEncoding");
  assert!(serde_json::from_value::<GeodisplayMetadata>(unsupported_encoding).is_err());

  let mut unsupported_geometry = metadata;
  unsupported_geometry["geometryType"] = json!("futureGeometry");
  assert!(serde_json::from_value::<GeodisplayMetadata>(unsupported_geometry).is_err());
}

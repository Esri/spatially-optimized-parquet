use spatial::geometry::Extent2D;
use spatial::optimized::metadata::{
  DisplayIndexXz, DisplayIndexXzInput, DisplayIndexZ, DisplayIndexZInput, GeodisplayMetadata,
};
use spatial::optimized::multiscale::{MultiscaleLevel, QuantizationTransform};

#[test]
fn point_metadata_serializes_spec_keys() {
  let metadata = GeodisplayMetadata::point(DisplayIndexZ::new(DisplayIndexZInput {
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
  }));

  let value = serde_json::to_value(metadata).unwrap();
  let index = &value["index"];
  assert_eq!(index["type"], "z");
  assert_eq!(index["code"], "zCode");
  assert_eq!(index["xColumn"], "x");
  assert_eq!(index["yColumn"], "y");
  assert_eq!(index["coordinatePrecision"], 20);
  assert_eq!(index["hasZ"], false);
  assert_eq!(index["hasM"], false);
}

#[test]
fn xz_metadata_serializes_bounds_and_levels() {
  let metadata = GeodisplayMetadata::xz_with_parent(
    "geodisplay",
    DisplayIndexXz::new(DisplayIndexXzInput {
      code: "xzCode".to_string(),
      encoding: "pbf".to_string(),
      geometry_type: "polygon".to_string(),
      bounds: "bounds".to_string(),
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
      levels: vec![MultiscaleLevel {
        column: "esri-multiscale-0".into(),
        level: 0,
        resolution: 0.70312359375,
        scale: 295_828_763.7958547,
        transform: QuantizationTransform {
          scale: [0.70312359375, 0.70312359375, 1.0, 1.0],
          translate: [0.0, 0.0, 0.0, 0.0],
        },
      }],
    }),
  );

  let value = serde_json::to_value(metadata).unwrap();
  assert_eq!(value["parentColumn"], "geodisplay");
  let index = &value["index"];
  assert_eq!(index["type"], "xz");
  assert_eq!(index["bounds"], "bounds");
  assert_eq!(index["encoding"], "pbf");
  assert_eq!(index["geometryType"], "polygon");
  assert_eq!(index["maxLevel"], 20);
  assert_eq!(index["levels"][0]["column"], "esri-multiscale-0");
  assert_eq!(index["wkid"], 4326);
  assert_eq!(index["levels"][0]["resolution"], 0.70312359375);
  assert_eq!(index["levels"][0]["scale"], 295828763.7958547);
}

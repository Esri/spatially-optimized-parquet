use spatial::analysis::Extent2D;
use spatial::metadata::output::{
  DisplayIndexXz, DisplayIndexZ, GeodisplayMetadata, MultiscaleLevel, QuantizationTransform,
};

#[test]
fn point_metadata_serializes_spec_keys() {
  let metadata = GeodisplayMetadata::point(DisplayIndexZ::new(
    "zCode",
    "x",
    "y",
    20,
    Extent2D {
      xmin: -180.0,
      ymin: -90.0,
      xmax: 180.0,
      ymax: 90.0,
    },
    Some(4326),
    None,
    false,
    false,
  ));

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
    DisplayIndexXz::new(
      "xzCode",
      "pbf",
      "polygon",
      "bounds",
      Extent2D {
        xmin: -10.0,
        ymin: -5.0,
        xmax: 10.0,
        ymax: 5.0,
      },
      20,
      Some(4326),
      None,
      false,
      false,
      vec![MultiscaleLevel {
        column: "esri-multiscale-0".into(),
        level: 0,
        resolution: 0.70312359375,
        scale: 295_828_763.7958547,
        transform: QuantizationTransform {
          scale: [0.70312359375, 0.70312359375, 1.0, 1.0],
          translate: [0.0, 0.0, 0.0, 0.0],
        },
      }],
    ),
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

mod common;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow_array::{Array, BinaryArray, Int32Array, RecordBatch, StringArray, StructArray};
use arrow_schema::{DataType, Field, Schema};
use parquet::basic::Compression;
use prost::Message;
use spatial::{
  InputOptions, OutputMode, OutputOptions, RowRange, SpatialPipelineOptions, run, validate,
};
use tokio::runtime::Runtime;

use common::fixture::{
  wkb_dimensional_line_string, wkb_dimensional_multi_point, wkb_dimensional_point,
  wkb_dimensional_polygon_rings,
};
use common::parquet::{geoparquet_kv, kv_map, scan_parquet, write_parquet};
use common::wkb::DimensionalCoordinate;

const FIXTURE_DIRECTORY: &str = "target/dimensional-sop-fixtures";
const INPUT_DIRECTORY: &str = "target/dimensional-sop-fixture-input";

#[derive(Clone, Copy, Debug)]
enum CoordinateLayout {
  Xyz,
  Xym,
  Xyzm,
}

impl CoordinateLayout {
  const ALL: [Self; 3] = [Self::Xyz, Self::Xym, Self::Xyzm];

  fn suffix(self) -> &'static str {
    match self {
      Self::Xyz => "xyz",
      Self::Xym => "xym",
      Self::Xyzm => "xyzm",
    }
  }

  fn geometry_suffix(self) -> &'static str {
    match self {
      Self::Xyz => "Z",
      Self::Xym => "M",
      Self::Xyzm => "ZM",
    }
  }

  fn has_z(self) -> bool {
    matches!(self, Self::Xyz | Self::Xyzm)
  }

  fn has_m(self) -> bool {
    matches!(self, Self::Xym | Self::Xyzm)
  }

  fn stride(self) -> usize {
    2 + usize::from(self.has_z()) + usize::from(self.has_m())
  }

  fn coordinate(self, x: f64, y: f64, z: f64, m: f64) -> DimensionalCoordinate {
    (x, y, self.has_z().then_some(z), self.has_m().then_some(m))
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FixtureGeometry {
  Point,
  MultiPoint,
  Polyline,
  Polygon,
}

impl FixtureGeometry {
  const ALL: [Self; 4] = [Self::Point, Self::MultiPoint, Self::Polyline, Self::Polygon];

  fn file_name(self, layout: CoordinateLayout) -> String {
    format!("dimensional-{}-{}.parquet", self.name(), layout.suffix())
  }

  fn name(self) -> &'static str {
    match self {
      Self::Point => "point",
      Self::MultiPoint => "multipoint",
      Self::Polyline => "polyline",
      Self::Polygon => "polygon",
    }
  }

  fn geoparquet_type(self, layout: CoordinateLayout) -> String {
    let base = match self {
      Self::Point => "Point",
      Self::MultiPoint => "MultiPoint",
      Self::Polyline => "LineString",
      Self::Polygon => "Polygon",
    };
    format!("{base} {}", layout.geometry_suffix())
  }
}

#[derive(Clone, PartialEq, Message)]
struct PbfGeometry {
  #[prost(uint32, repeated, tag = "2")]
  lengths: Vec<u32>,
  #[prost(sint64, repeated, tag = "3")]
  coords: Vec<i64>,
}

#[test]
#[ignore = "writes deterministic integration fixtures under target/"]
fn generate_dimensional_sop_fixtures() {
  let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
  let fixture_directory = repository.join(FIXTURE_DIRECTORY);
  let input_directory = repository.join(INPUT_DIRECTORY);
  reset_directory(&fixture_directory);
  reset_directory(&input_directory);

  for geometry in FixtureGeometry::ALL {
    for layout in CoordinateLayout::ALL {
      let file_name = geometry.file_name(layout);
      let input = input_directory.join(&file_name);
      let output = fixture_directory.join(&file_name);
      write_input_fixture(&input, geometry, layout);
      optimize_fixture(&input, &output);
      inspect_fixture(&output, geometry, layout);
      println!("{}", output.display());
    }
  }

  fs::remove_dir_all(input_directory).unwrap();
}

#[test]
fn optimized_dimensional_fixture_matrix_validates() {
  let temp = tempfile::TempDir::new().unwrap();
  for geometry in FixtureGeometry::ALL {
    for layout in CoordinateLayout::ALL {
      let file_name = geometry.file_name(layout);
      let input = temp.path().join(format!("input-{file_name}"));
      let output = temp.path().join(file_name);
      write_input_fixture(&input, geometry, layout);
      optimize_fixture(&input, &output);
      inspect_fixture(&output, geometry, layout);
    }
  }
}

fn reset_directory(path: &Path) {
  if path.exists() {
    fs::remove_dir_all(path).unwrap();
  }
  fs::create_dir_all(path).unwrap();
}

fn write_input_fixture(path: &Path, geometry: FixtureGeometry, layout: CoordinateLayout) {
  let schema = Arc::new(Schema::new(vec![
    Field::new("id", DataType::Int32, false),
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, false),
  ]));
  let geometries = fixture_geometries(geometry, layout);
  let geometry_values = geometries
    .iter()
    .map(|geometry| Some(geometry.as_slice()))
    .collect::<Vec<_>>();
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(Int32Array::from(vec![1, 2])),
      Arc::new(StringArray::from(vec!["feature-1", "feature-2"])),
      Arc::new(BinaryArray::from(geometry_values)),
    ],
  )
  .unwrap();
  let geometry_type = geometry.geoparquet_type(layout);
  write_parquet(
    path,
    &schema,
    &[batch],
    Compression::SNAPPY,
    &[geoparquet_kv("geometry", &[geometry_type.as_str()])],
  );
}

fn optimize_fixture(input: &Path, output: &Path) {
  Runtime::new()
    .unwrap()
    .block_on(run(SpatialPipelineOptions::new(
      InputOptions::new(
        input.to_string_lossy(),
        None,
        RowRange::default(),
        None,
        None,
        None,
      ),
      OutputOptions::new(
        output,
        OutputMode::OptimizedGeoParquet,
        None,
        None,
        4326,
        false,
        true,
      ),
    )))
    .unwrap();
}

fn inspect_fixture(path: &Path, geometry: FixtureGeometry, layout: CoordinateLayout) {
  let report = validate(path).unwrap();
  assert!(
    !report.has_errors(),
    "{} failed SOP validation: {report:#?}",
    path.display()
  );

  let metadata = kv_map(path);
  let geo: serde_json::Value = serde_json::from_str(metadata.get("geo").unwrap()).unwrap();
  let geodisplay: serde_json::Value =
    serde_json::from_str(metadata.get("geodisplay").unwrap()).unwrap();
  assert_eq!(
    geo["columns"]["geometry"]["geometry_types"][0],
    geometry.geoparquet_type(layout)
  );
  assert_eq!(geodisplay["index"]["hasZ"], layout.has_z());
  assert_eq!(geodisplay["index"]["hasM"], layout.has_m());

  let runtime = Runtime::new().unwrap();
  let dataframe = runtime
    .block_on(scan_parquet(path.to_str().unwrap()))
    .unwrap();
  let batches = runtime.block_on(dataframe.collect()).unwrap();
  assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(), 2);

  if geometry == FixtureGeometry::Point {
    inspect_point_columns(&batches, layout);
  } else {
    inspect_multiscale_geometry(&batches, geometry, layout);
  }
}

fn inspect_point_columns(batches: &[RecordBatch], layout: CoordinateLayout) {
  let batch = &batches[0];
  let geodisplay = batch
    .column_by_name("geodisplay")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  assert_eq!(geodisplay.column_by_name("z").is_some(), layout.has_z());
  assert_eq!(geodisplay.column_by_name("m").is_some(), layout.has_m());
  assert!(geodisplay.column_by_name("level_16").is_none());
}

fn inspect_multiscale_geometry(
  batches: &[RecordBatch],
  geometry: FixtureGeometry,
  layout: CoordinateLayout,
) {
  for batch in batches {
    let geodisplay = batch
      .column_by_name("geodisplay")
      .unwrap()
      .as_any()
      .downcast_ref::<StructArray>()
      .unwrap();
    let level = geodisplay
      .column_by_name("level_16")
      .unwrap()
      .as_any()
      .downcast_ref::<BinaryArray>()
      .unwrap();
    for row in 0..batch.num_rows() {
      let decoded = PbfGeometry::decode(level.value(row)).unwrap();
      let vertex_count = decoded
        .lengths
        .iter()
        .map(|length| *length as usize)
        .sum::<usize>();
      assert_eq!(decoded.coords.len(), vertex_count * layout.stride());
      if geometry == FixtureGeometry::Polygon {
        assert_eq!(decoded.lengths, vec![5, 5]);
        assert_representative_polygon_components(&decoded, layout);
      }
    }
  }
}

fn assert_representative_polygon_components(decoded: &PbfGeometry, layout: CoordinateLayout) {
  let coordinates = decoded
    .coords
    .chunks_exact(layout.stride())
    .collect::<Vec<_>>();
  if layout.has_z() {
    let z_index = 2;
    let minimum = coordinates
      .iter()
      .map(|coordinate| coordinate[z_index])
      .min()
      .unwrap();
    assert!(matches!(minimum, 111 | 211), "{coordinates:?}");
    assert!(
      coordinates
        .iter()
        .any(|coordinate| coordinate[z_index] == minimum + 40),
      "{coordinates:?}"
    );
  }
  if layout.has_m() {
    let m_index = if layout.has_z() { 3 } else { 2 };
    let minimum = coordinates
      .iter()
      .map(|coordinate| coordinate[m_index])
      .min()
      .unwrap();
    assert!(matches!(minimum, 1111 | 2111), "{coordinates:?}");
    assert!(
      coordinates
        .iter()
        .any(|coordinate| coordinate[m_index] == minimum + 40),
      "{coordinates:?}"
    );
  }
}

fn fixture_geometries(geometry: FixtureGeometry, layout: CoordinateLayout) -> [Vec<u8>; 2] {
  match geometry {
    FixtureGeometry::Point => [
      wkb_dimensional_point(
        -120.1,
        35.1,
        layout.has_z().then_some(101.0),
        layout.has_m().then_some(1001.0),
      ),
      wkb_dimensional_point(
        -118.2,
        37.2,
        layout.has_z().then_some(201.0),
        layout.has_m().then_some(2001.0),
      ),
    ],
    FixtureGeometry::MultiPoint => [
      wkb_dimensional_multi_point(&[
        layout.coordinate(-120.1, 35.1, 101.0, 1001.0),
        layout.coordinate(-119.9, 35.3, 102.0, 1002.0),
        layout.coordinate(-119.7, 35.5, 103.0, 1003.0),
      ]),
      wkb_dimensional_multi_point(&[
        layout.coordinate(-118.2, 37.2, 201.0, 2001.0),
        layout.coordinate(-118.0, 37.4, 202.0, 2002.0),
        layout.coordinate(-117.8, 37.6, 203.0, 2003.0),
      ]),
    ],
    FixtureGeometry::Polyline => [
      wkb_dimensional_line_string(&[
        layout.coordinate(-120.4, 35.0, 101.0, 1001.0),
        layout.coordinate(-120.1, 35.3, 102.0, 1002.0),
        layout.coordinate(-119.8, 35.1, 103.0, 1003.0),
        layout.coordinate(-119.5, 35.5, 104.0, 1004.0),
      ]),
      wkb_dimensional_line_string(&[
        layout.coordinate(-118.5, 37.0, 201.0, 2001.0),
        layout.coordinate(-118.2, 37.3, 202.0, 2002.0),
        layout.coordinate(-117.9, 37.1, 203.0, 2003.0),
        layout.coordinate(-117.6, 37.5, 204.0, 2004.0),
      ]),
    ],
    FixtureGeometry::Polygon => [
      polygon_fixture(layout, -120.5, 35.0, 111.0, 1111.0),
      polygon_fixture(layout, -118.5, 37.0, 211.0, 2111.0),
    ],
  }
}

fn polygon_fixture(
  layout: CoordinateLayout,
  x: f64,
  y: f64,
  component_start: f64,
  measure_start: f64,
) -> Vec<u8> {
  let exterior = [
    layout.coordinate(x, y, component_start, measure_start),
    layout.coordinate(x + 1.0, y, component_start + 10.0, measure_start + 10.0),
    layout.coordinate(
      x + 1.0,
      y + 1.0,
      component_start + 20.0,
      measure_start + 20.0,
    ),
    layout.coordinate(x, y + 1.0, component_start + 30.0, measure_start + 30.0),
    layout.coordinate(x, y, component_start, measure_start),
  ];
  let hole = [
    layout.coordinate(
      x + 0.25,
      y + 0.25,
      component_start + 40.0,
      measure_start + 40.0,
    ),
    layout.coordinate(
      x + 0.25,
      y + 0.75,
      component_start + 50.0,
      measure_start + 50.0,
    ),
    layout.coordinate(
      x + 0.75,
      y + 0.75,
      component_start + 60.0,
      measure_start + 60.0,
    ),
    layout.coordinate(
      x + 0.75,
      y + 0.25,
      component_start + 70.0,
      measure_start + 70.0,
    ),
    layout.coordinate(
      x + 0.25,
      y + 0.25,
      component_start + 40.0,
      measure_start + 40.0,
    ),
  ];
  wkb_dimensional_polygon_rings(&[&exterior, &hole])
}

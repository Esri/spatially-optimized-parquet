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

use std::collections::HashMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow_array::{
  Array, BinaryArray, BinaryViewArray, Int32Array, LargeBinaryArray, RecordBatch, StringArray,
  StructArray,
};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use gdal::spatial_ref::SpatialRef;
use parquet::arrow::arrow_reader::{
  ArrowReaderMetadata, ArrowReaderOptions, ParquetRecordBatchReaderBuilder,
};
use parquet::arrow::arrow_writer::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::metadata::KeyValue;
use parquet::file::properties::WriterProperties;
use prost::Message;
use spatial::{
  InputOptions, OutputMode, OutputOptions, Pipeline, PipelineError, SpatialPipelineOptions,
  ValidationError, ValidationRule, ValidationSeverity, validate,
};

const OUTPUT_DIRECTORY: &str = "test-data";
const SOP_DIRECTORY: &str = "sop";
const GEOPARQUET_EXTENSIONS_DIRECTORY: &str = "geoparquet-extensions";
const START_LONGITUDE: f64 = -117.195_645_8;
const START_LATITUDE: f64 = 34.055_953_3;
const NORMALIZATION_EXTENT: [f64; 4] = [-180.0, -90.0, 180.0, 90.0];
const CLUSTER_DEPTH: u32 = 20;

type Coordinate = (f64, f64, Option<f64>, Option<f64>);
type Result<T> = std::result::Result<T, FixtureError>;

#[derive(Debug, thiserror::Error)]
enum FixtureError {
  #[error("{operation}: {source}")]
  Io {
    operation: String,
    #[source]
    source: std::io::Error,
  },
  #[error("{operation}: {source}")]
  Arrow {
    operation: String,
    #[source]
    source: arrow_schema::ArrowError,
  },
  #[error("{operation}: {source}")]
  Parquet {
    operation: String,
    #[source]
    source: parquet::errors::ParquetError,
  },
  #[error("{operation}: {source}")]
  Gdal {
    operation: String,
    #[source]
    source: gdal::errors::GdalError,
  },
  #[error("{operation}: {source}")]
  Json {
    operation: String,
    #[source]
    source: serde_json::Error,
  },
  #[error("{operation}: {source}")]
  Pipeline {
    operation: String,
    #[source]
    source: PipelineError,
  },
  #[error("{operation}: {source}")]
  Validation {
    operation: String,
    #[source]
    source: ValidationError,
  },
  #[error("{operation}: {source}")]
  Protobuf {
    operation: String,
    #[source]
    source: prost::DecodeError,
  },
  #[error("{0}")]
  Check(String),
}

macro_rules! ensure_fixture {
  ($condition:expr $(,)?) => {
    if !$condition {
      return Err(FixtureError::Check(format!(
        "fixture invariant failed: {}",
        stringify!($condition)
      )));
    }
  };
  ($condition:expr, $($argument:tt)+) => {
    if !$condition {
      return Err(FixtureError::Check(format!($($argument)+)));
    }
  };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CoordinateLayout {
  Xy,
  Xyz,
  Xym,
  Xyzm,
}

impl CoordinateLayout {
  const ALL: [Self; 4] = [Self::Xy, Self::Xyz, Self::Xym, Self::Xyzm];

  fn suffix(self) -> &'static str {
    match self {
      Self::Xy => "xy",
      Self::Xyz => "xyz",
      Self::Xym => "xym",
      Self::Xyzm => "xyzm",
    }
  }

  fn geometry_suffix(self) -> &'static str {
    match self {
      Self::Xy => "",
      Self::Xyz => " Z",
      Self::Xym => " M",
      Self::Xyzm => " ZM",
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

  fn coordinate(self, index: usize, longitude: f64, latitude: f64) -> Coordinate {
    (
      longitude,
      latitude,
      self.has_z().then_some(100.0 + index as f64 * 0.25),
      self.has_m().then_some(1_000.0 + index as f64),
    )
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FixtureGeometry {
  Point,
  MultiPoint,
  LineString,
  MultiLineString,
  Polygon,
  MultiPolygon,
}

impl FixtureGeometry {
  const ALL: [Self; 6] = [
    Self::Point,
    Self::MultiPoint,
    Self::LineString,
    Self::MultiLineString,
    Self::Polygon,
    Self::MultiPolygon,
  ];

  fn name(self) -> &'static str {
    match self {
      Self::Point => "point",
      Self::MultiPoint => "multipoint",
      Self::LineString => "linestring",
      Self::MultiLineString => "multilinestring",
      Self::Polygon => "polygon",
      Self::MultiPolygon => "multipolygon",
    }
  }

  fn geoparquet_type(self, layout: CoordinateLayout) -> String {
    let geometry_type = match self {
      Self::Point => "Point",
      Self::MultiPoint => "MultiPoint",
      Self::LineString => "LineString",
      Self::MultiLineString => "MultiLineString",
      Self::Polygon => "Polygon",
      Self::MultiPolygon => "MultiPolygon",
    };
    format!("{geometry_type}{}", layout.geometry_suffix())
  }

  fn is_complex(self) -> bool {
    matches!(
      self,
      Self::LineString | Self::MultiLineString | Self::Polygon | Self::MultiPolygon
    )
  }

  fn source_vertex_count(self) -> usize {
    match self {
      Self::LineString | Self::MultiLineString => 6,
      Self::Polygon => 10,
      Self::MultiPolygon => 15,
      Self::Point | Self::MultiPoint => 0,
    }
  }

  fn wkb_type(self) -> u32 {
    match self {
      Self::Point => 1,
      Self::MultiPoint => 4,
      Self::LineString => 2,
      Self::MultiLineString => 5,
      Self::Polygon => 3,
      Self::MultiPolygon => 6,
    }
  }
}

#[derive(Clone, Copy)]
enum OutputContract {
  Sop,
  GeoparquetExtensions,
}

impl OutputContract {
  fn directory(self) -> &'static str {
    match self {
      Self::Sop => SOP_DIRECTORY,
      Self::GeoparquetExtensions => GEOPARQUET_EXTENSIONS_DIRECTORY,
    }
  }
}

#[derive(Clone, PartialEq, Message)]
struct PbfGeometry {
  #[prost(uint32, repeated, tag = "2")]
  lengths: Vec<u32>,
  #[prost(sint64, repeated, tag = "3")]
  coords: Vec<i64>,
}

fn main() -> Result<()> {
  let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
  let output_root = repository.join(OUTPUT_DIRECTORY);
  let input_directory = tempfile::tempdir().map_err(|source| FixtureError::Io {
    operation: "create temporary fixture input directory".to_string(),
    source,
  })?;

  let sop_directory = output_root.join(SOP_DIRECTORY);
  let extensions_directory = output_root.join(GEOPARQUET_EXTENSIONS_DIRECTORY);
  reset_directory(&sop_directory)?;
  reset_directory(&extensions_directory)?;

  let runtime = tokio::runtime::Runtime::new().map_err(|source| FixtureError::Io {
    operation: "create Tokio runtime".to_string(),
    source,
  })?;
  generate_sop(&runtime, input_directory.path(), &sop_directory)?;
  generate_geoparquet_extensions(&runtime, input_directory.path(), &extensions_directory)?;
  Ok(())
}

fn generate_sop(
  runtime: &tokio::runtime::Runtime,
  input_directory: &Path,
  output_directory: &Path,
) -> Result<()> {
  generate_family(
    runtime,
    input_directory,
    output_directory,
    OutputContract::Sop,
  )
}

fn generate_geoparquet_extensions(
  runtime: &tokio::runtime::Runtime,
  input_directory: &Path,
  output_directory: &Path,
) -> Result<()> {
  generate_family(
    runtime,
    input_directory,
    output_directory,
    OutputContract::GeoparquetExtensions,
  )
}

fn generate_family(
  runtime: &tokio::runtime::Runtime,
  input_directory: &Path,
  output_directory: &Path,
  contract: OutputContract,
) -> Result<()> {
  for layout in CoordinateLayout::ALL {
    for geometry in FixtureGeometry::ALL {
      let file_name = format!("{}-{}.parquet", geometry.name(), layout.suffix());
      let input = input_directory.join(format!("{}-{file_name}", contract.directory()));
      let output = output_directory.join(file_name);
      write_input_fixture(&input, geometry, layout)?;
      optimize_fixture(runtime, &input, &output, contract)?;
      inspect_fixture(&output, geometry, layout, contract)?;
      println!("{}", output.display());
    }
  }
  Ok(())
}

fn reset_directory(path: &Path) -> Result<()> {
  if path.exists() {
    fs::remove_dir_all(path).map_err(|source| FixtureError::Io {
      operation: format!("remove existing fixture directory {}", path.display()),
      source,
    })?;
  }
  fs::create_dir_all(path).map_err(|source| FixtureError::Io {
    operation: format!("create fixture directory {}", path.display()),
    source,
  })
}

fn write_input_fixture(
  path: &Path,
  geometry: FixtureGeometry,
  layout: CoordinateLayout,
) -> Result<()> {
  let schema = Arc::new(Schema::new(vec![
    Field::new("id", DataType::Int32, false),
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, false),
  ]));
  let geometry_value = fixture_geometry(geometry, layout);
  let batch = RecordBatch::try_new(
    Arc::clone(&schema),
    vec![
      Arc::new(Int32Array::from(vec![1])),
      Arc::new(StringArray::from(vec!["feature"])),
      Arc::new(BinaryArray::from(vec![Some(geometry_value.as_slice())])),
    ],
  )
  .map_err(|source| FixtureError::Arrow {
    operation: "construct fixture record batch".to_string(),
    source,
  })?;
  let geometry_type = geometry.geoparquet_type(layout);
  write_parquet(
    path,
    &schema,
    &[batch],
    &[geoparquet_metadata("geometry", &[geometry_type.as_str()])?],
  )
}

fn optimize_fixture(
  runtime: &tokio::runtime::Runtime,
  input: &Path,
  output: &Path,
  contract: OutputContract,
) -> Result<()> {
  let (write_sop, write_extensions) = match contract {
    OutputContract::Sop => (true, false),
    OutputContract::GeoparquetExtensions => (false, true),
  };
  runtime
    .block_on(Pipeline::run(SpatialPipelineOptions {
      input: InputOptions {
        location: input.to_string_lossy().into_owned(),
        ..Default::default()
      },
      output: OutputOptions {
        path: output.to_path_buf(),
        mode: OutputMode::Optimized,
        overwrite: true,
        write_sop,
        write_extensions,
        normalization_extent: Some(NORMALIZATION_EXTENT),
        cluster_depth: CLUSTER_DEPTH,
        ..Default::default()
      },
      ..Default::default()
    }))
    .map_err(|source| FixtureError::Pipeline {
      operation: format!("optimize fixture {}", output.display()),
      source,
    })?;
  Ok(())
}

fn inspect_fixture(
  path: &Path,
  geometry: FixtureGeometry,
  layout: CoordinateLayout,
  contract: OutputContract,
) -> Result<()> {
  let report = validate(path).map_err(|source| FixtureError::Validation {
    operation: format!("validate {}", path.display()),
    source,
  })?;
  match contract {
    OutputContract::Sop => ensure_fixture!(
      !report.has_errors(),
      "{} failed validation: {report:#?}",
      path.display()
    ),
    OutputContract::GeoparquetExtensions => ensure_fixture!(
      report.findings().iter().all(|finding| {
        finding.severity() != ValidationSeverity::Error
          || (finding.rule() == ValidationRule::MetadataMissing
            && finding.message().contains("'geodisplay'"))
      }),
      "{} has unexpected validation errors: {report:#?}",
      path.display()
    ),
  }

  let metadata = parquet_metadata(path)?;
  let geo: serde_json::Value = serde_json::from_str(metadata.get("geo").ok_or_else(|| {
    FixtureError::Check("generated fixture is missing GeoParquet metadata".to_string())
  })?)
  .map_err(|source| FixtureError::Json {
    operation: "parse generated GeoParquet metadata".to_string(),
    source,
  })?;
  ensure_fixture!(
    geo["columns"]["geometry"]["geometry_types"][0] == geometry.geoparquet_type(layout),
    "{} has unexpected geometry metadata",
    path.display()
  );

  match contract {
    OutputContract::Sop => inspect_sop_metadata(path, &metadata, layout)?,
    OutputContract::GeoparquetExtensions => {
      inspect_geoparquet_extension_metadata(path, &metadata, &geo, geometry)?
    }
  }

  let batches = read_batches(path)?;
  ensure_fixture!(
    batches.iter().map(RecordBatch::num_rows).sum::<usize>() == 1,
    "{} must contain one feature",
    path.display()
  );
  inspect_wkb_layout(&batches, geometry, layout, path)?;
  if geometry.is_complex() {
    inspect_simplification(&batches, geometry, layout, path)?;
  } else if geometry == FixtureGeometry::Point {
    inspect_point_components(&batches, layout, path)?;
  }
  Ok(())
}

fn inspect_wkb_layout(
  batches: &[RecordBatch],
  geometry: FixtureGeometry,
  layout: CoordinateLayout,
  path: &Path,
) -> Result<()> {
  let geometry_array = batches[0]
    .column_by_name("geometry")
    .ok_or_else(|| FixtureError::Check(format!("{} is missing geometry", path.display())))?;
  let bytes = binary_value(geometry_array.as_ref(), 0).ok_or_else(|| {
    FixtureError::Check(format!("{} geometry must be binary WKB", path.display()))
  })?;
  ensure_fixture!(
    bytes.first() == Some(&1),
    "{} must use little-endian WKB",
    path.display()
  );
  let encoded_type = u32::from_le_bytes(
    bytes
      .get(1..5)
      .ok_or_else(|| FixtureError::Check("geometry WKB is truncated".to_string()))?
      .try_into()
      .expect("WKB type occupies four bytes"),
  );
  let expected_type = geometry_type(geometry.wkb_type(), layout.has_z(), layout.has_m());
  ensure_fixture!(
    encoded_type == expected_type,
    "{} has WKB type {encoded_type}, expected {expected_type}",
    path.display()
  );
  Ok(())
}

fn binary_value(array: &dyn Array, index: usize) -> Option<&[u8]> {
  if let Some(array) = array.as_any().downcast_ref::<BinaryArray>() {
    return Some(array.value(index));
  }
  if let Some(array) = array.as_any().downcast_ref::<LargeBinaryArray>() {
    return Some(array.value(index));
  }
  if let Some(array) = array.as_any().downcast_ref::<BinaryViewArray>() {
    return Some(array.value(index));
  }
  None
}

fn inspect_sop_metadata(
  path: &Path,
  metadata: &HashMap<String, String>,
  layout: CoordinateLayout,
) -> Result<()> {
  let geodisplay: serde_json::Value = serde_json::from_str(
    metadata
      .get("geodisplay")
      .ok_or_else(|| FixtureError::Check(format!("{} is missing SOP metadata", path.display())))?,
  )
  .map_err(|source| FixtureError::Json {
    operation: "parse SOP metadata".to_string(),
    source,
  })?;
  ensure_fixture!(geodisplay["hasZ"] == layout.has_z());
  ensure_fixture!(geodisplay["hasM"] == layout.has_m());
  Ok(())
}

fn inspect_geoparquet_extension_metadata(
  path: &Path,
  metadata: &HashMap<String, String>,
  geo: &serde_json::Value,
  geometry: FixtureGeometry,
) -> Result<()> {
  ensure_fixture!(
    !metadata.contains_key("geodisplay"),
    "{} unexpectedly contains SOP metadata",
    path.display()
  );
  ensure_fixture!(
    geo.get("ordering").is_some(),
    "{} is missing GeoParquet ordering metadata",
    path.display()
  );
  ensure_fixture!(
    geo.get("lod").is_some() == geometry.is_complex(),
    "{} has unexpected GeoParquet LOD metadata",
    path.display()
  );
  Ok(())
}

fn inspect_point_components(
  batches: &[RecordBatch],
  layout: CoordinateLayout,
  path: &Path,
) -> Result<()> {
  let sop_geometry = batches[0]
    .column_by_name("sop_geometry")
    .ok_or_else(|| FixtureError::Check(format!("{} is missing sop_geometry", path.display())))?
    .as_any()
    .downcast_ref::<StructArray>()
    .ok_or_else(|| FixtureError::Check("sop_geometry must be a struct".to_string()))?;
  ensure_fixture!(sop_geometry.column_by_name("z").is_some() == layout.has_z());
  ensure_fixture!(sop_geometry.column_by_name("m").is_some() == layout.has_m());
  Ok(())
}

fn inspect_simplification(
  batches: &[RecordBatch],
  geometry: FixtureGeometry,
  layout: CoordinateLayout,
  path: &Path,
) -> Result<()> {
  let source_vertex_count = geometry.source_vertex_count();
  let mut saw_reduced_level = false;
  for batch in batches {
    let geolod = batch
      .column_by_name("geolod")
      .ok_or_else(|| FixtureError::Check(format!("{} is missing geolod", path.display())))?
      .as_any()
      .downcast_ref::<StructArray>()
      .ok_or_else(|| FixtureError::Check("geolod must be a struct".to_string()))?;
    for column in geolod.columns() {
      let level = column
        .as_any()
        .downcast_ref::<BinaryArray>()
        .ok_or_else(|| FixtureError::Check("PBF geolod level must be binary".to_string()))?;
      for row in 0..level.len() {
        if level.is_null(row) {
          continue;
        }
        let decoded =
          PbfGeometry::decode(level.value(row)).map_err(|source| FixtureError::Protobuf {
            operation: "decode PBF geolod level".to_string(),
            source,
          })?;
        let vertex_count = decoded
          .lengths
          .iter()
          .map(|length| *length as usize)
          .sum::<usize>();
        ensure_fixture!(
          decoded.coords.len() == vertex_count * layout.stride(),
          "{} has an invalid coordinate stride",
          path.display()
        );
        saw_reduced_level |= vertex_count < source_vertex_count;
      }
    }
  }
  ensure_fixture!(
    saw_reduced_level,
    "{} did not produce a simplified lower-detail level",
    path.display()
  );
  Ok(())
}

fn fixture_geometry(geometry: FixtureGeometry, layout: CoordinateLayout) -> Vec<u8> {
  match geometry {
    FixtureGeometry::Point => {
      let (longitude, latitude, z, m) = layout.coordinate(0, START_LONGITUDE, START_LATITUDE);
      encode_point(longitude, latitude, z, m)
    }
    FixtureGeometry::MultiPoint => encode_multi_point(&[
      offset_coordinate(layout, 0, 0.000, 0.000),
      offset_coordinate(layout, 1, 0.003, 0.002),
      offset_coordinate(layout, 2, 0.006, -0.001),
      offset_coordinate(layout, 3, 0.009, 0.003),
    ]),
    FixtureGeometry::LineString => encode_line_string(&redlands_line_string(layout)),
    FixtureGeometry::MultiLineString => {
      encode_multi_line_string(&redlands_multi_line_string(layout))
    }
    FixtureGeometry::Polygon => encode_polygon(&redlands_polygon(layout)),
    FixtureGeometry::MultiPolygon => {
      let polygons = redlands_multi_polygon(layout);
      encode_multi_polygon(&polygons)
    }
  }
}

fn redlands_line_string(layout: CoordinateLayout) -> Vec<Coordinate> {
  [
    (0.000, 0.000),
    (0.002, 0.002),
    (0.004, -0.001),
    (0.006, 0.003),
    (0.008, 0.000),
    (0.010, 0.002),
  ]
  .into_iter()
  .enumerate()
  .map(|(index, (longitude_offset, latitude_offset))| {
    offset_coordinate(layout, index, longitude_offset, latitude_offset)
  })
  .collect()
}

fn redlands_multi_line_string(layout: CoordinateLayout) -> Vec<Vec<Coordinate>> {
  let line = redlands_line_string(layout);
  vec![line[..3].to_vec(), line[3..].to_vec()]
}

fn redlands_multi_polygon(layout: CoordinateLayout) -> Vec<Vec<Vec<Coordinate>>> {
  vec![
    redlands_polygon(layout),
    vec![offset_ring(
      layout,
      10,
      &[
        (0.012, -0.001),
        (0.012, 0.003),
        (0.018, 0.003),
        (0.018, -0.001),
        (0.012, -0.001),
      ],
    )],
  ]
}

fn redlands_polygon(layout: CoordinateLayout) -> Vec<Vec<Coordinate>> {
  vec![
    offset_ring(
      layout,
      0,
      &[
        (0.000, -0.002),
        (0.000, 0.004),
        (0.008, 0.004),
        (0.008, -0.002),
        (0.000, -0.002),
      ],
    ),
    offset_ring(
      layout,
      5,
      &[
        (0.002, 0.000),
        (0.006, 0.000),
        (0.006, 0.002),
        (0.002, 0.002),
        (0.002, 0.000),
      ],
    ),
  ]
}

fn offset_coordinate(
  layout: CoordinateLayout,
  index: usize,
  longitude_offset: f64,
  latitude_offset: f64,
) -> Coordinate {
  layout.coordinate(
    index,
    START_LONGITUDE + longitude_offset,
    START_LATITUDE + latitude_offset,
  )
}

fn offset_ring(
  layout: CoordinateLayout,
  index_offset: usize,
  offsets: &[(f64, f64)],
) -> Vec<Coordinate> {
  offsets
    .iter()
    .enumerate()
    .map(|(index, (longitude_offset, latitude_offset))| {
      offset_coordinate(
        layout,
        index_offset + index,
        *longitude_offset,
        *latitude_offset,
      )
    })
    .collect()
}

fn encode_point(x: f64, y: f64, z: Option<f64>, m: Option<f64>) -> Vec<u8> {
  let mut output = Vec::with_capacity(37);
  write_header(&mut output, geometry_type(1, z.is_some(), m.is_some()));
  write_coordinate(&mut output, (x, y, z, m));
  output
}

fn encode_multi_point(coordinates: &[Coordinate]) -> Vec<u8> {
  let (has_z, has_m) = dimensions(coordinates);
  let mut output = Vec::new();
  write_header(&mut output, geometry_type(4, has_z, has_m));
  write_count(&mut output, coordinates.len());
  for coordinate in coordinates {
    write_header(&mut output, geometry_type(1, has_z, has_m));
    write_coordinate(&mut output, *coordinate);
  }
  output
}

fn encode_line_string(coordinates: &[Coordinate]) -> Vec<u8> {
  let (has_z, has_m) = dimensions(coordinates);
  let mut output = Vec::new();
  write_header(&mut output, geometry_type(2, has_z, has_m));
  write_count(&mut output, coordinates.len());
  for coordinate in coordinates {
    write_coordinate(&mut output, *coordinate);
  }
  output
}

fn encode_multi_line_string(lines: &[Vec<Coordinate>]) -> Vec<u8> {
  let coordinates = lines
    .iter()
    .flat_map(|line| line.iter())
    .copied()
    .collect::<Vec<_>>();
  let (has_z, has_m) = dimensions(&coordinates);
  let mut output = Vec::new();
  write_header(&mut output, geometry_type(5, has_z, has_m));
  write_count(&mut output, lines.len());
  for line in lines {
    write_header(&mut output, geometry_type(2, has_z, has_m));
    write_count(&mut output, line.len());
    for coordinate in line {
      write_coordinate(&mut output, *coordinate);
    }
  }
  output
}

fn encode_polygon(rings: &[Vec<Coordinate>]) -> Vec<u8> {
  let coordinates = rings
    .iter()
    .flat_map(|ring| ring.iter())
    .copied()
    .collect::<Vec<_>>();
  let (has_z, has_m) = dimensions(&coordinates);
  let mut output = Vec::new();
  write_header(&mut output, geometry_type(3, has_z, has_m));
  write_count(&mut output, rings.len());
  for ring in rings {
    write_count(&mut output, ring.len());
    for coordinate in ring {
      write_coordinate(&mut output, *coordinate);
    }
  }
  output
}

fn encode_multi_polygon(polygons: &[Vec<Vec<Coordinate>>]) -> Vec<u8> {
  let coordinates = polygons
    .iter()
    .flat_map(|polygon| polygon.iter())
    .flat_map(|ring| ring.iter())
    .copied()
    .collect::<Vec<_>>();
  let (has_z, has_m) = dimensions(&coordinates);
  let mut output = Vec::new();
  write_header(&mut output, geometry_type(6, has_z, has_m));
  write_count(&mut output, polygons.len());
  for polygon in polygons {
    write_header(&mut output, geometry_type(3, has_z, has_m));
    write_count(&mut output, polygon.len());
    for ring in polygon {
      write_count(&mut output, ring.len());
      for coordinate in ring {
        write_coordinate(&mut output, *coordinate);
      }
    }
  }
  output
}

fn write_header(output: &mut Vec<u8>, geometry_type: u32) {
  output.push(1);
  output.extend_from_slice(&geometry_type.to_le_bytes());
}

fn write_count(output: &mut Vec<u8>, count: usize) {
  output.extend_from_slice(
    &u32::try_from(count)
      .expect("fixture coordinate count fits u32")
      .to_le_bytes(),
  );
}

fn write_coordinate(output: &mut Vec<u8>, coordinate: Coordinate) {
  output.extend_from_slice(&coordinate.0.to_le_bytes());
  output.extend_from_slice(&coordinate.1.to_le_bytes());
  if let Some(z) = coordinate.2 {
    output.extend_from_slice(&z.to_le_bytes());
  }
  if let Some(m) = coordinate.3 {
    output.extend_from_slice(&m.to_le_bytes());
  }
}

fn dimensions(coordinates: &[Coordinate]) -> (bool, bool) {
  (
    coordinates.iter().any(|coordinate| coordinate.2.is_some()),
    coordinates.iter().any(|coordinate| coordinate.3.is_some()),
  )
}

fn geometry_type(base_type: u32, has_z: bool, has_m: bool) -> u32 {
  base_type
    + match (has_z, has_m) {
      (false, false) => 0,
      (true, false) => 1_000,
      (false, true) => 2_000,
      (true, true) => 3_000,
    }
}

fn write_parquet(
  path: &Path,
  schema: &SchemaRef,
  batches: &[RecordBatch],
  metadata: &[KeyValue],
) -> Result<()> {
  let properties = WriterProperties::builder()
    .set_compression(Compression::SNAPPY)
    .build();
  let mut writer = ArrowWriter::try_new(
    File::create(path).map_err(|source| FixtureError::Io {
      operation: format!("create {}", path.display()),
      source,
    })?,
    Arc::clone(schema),
    Some(properties),
  )
  .map_err(|source| FixtureError::Parquet {
    operation: "create Parquet writer".to_string(),
    source,
  })?;
  for batch in batches {
    writer
      .write(batch)
      .map_err(|source| FixtureError::Parquet {
        operation: "write fixture batch".to_string(),
        source,
      })?;
  }
  for entry in metadata {
    writer.append_key_value_metadata(entry.clone());
  }
  writer.close().map_err(|source| FixtureError::Parquet {
    operation: "close fixture Parquet writer".to_string(),
    source,
  })?;
  Ok(())
}

fn geoparquet_metadata(primary_column: &str, geometry_types: &[&str]) -> Result<KeyValue> {
  let crs = SpatialRef::from_epsg(4326)
    .map_err(|source| FixtureError::Gdal {
      operation: "load EPSG:4326".to_string(),
      source,
    })?
    .to_projjson()
    .map_err(|source| FixtureError::Gdal {
      operation: "serialize EPSG:4326".to_string(),
      source,
    })?;
  let crs: serde_json::Value = serde_json::from_str(&crs).map_err(|source| FixtureError::Json {
    operation: "parse EPSG:4326 PROJJSON".to_string(),
    source,
  })?;
  let value = serde_json::json!({
    "version": "1.1.0",
    "primary_column": primary_column,
    "columns": {
      primary_column: {
        "encoding": "WKB",
        "geometry_types": geometry_types,
        "crs": crs
      }
    }
  });
  Ok(KeyValue::new("geo".to_string(), Some(value.to_string())))
}

fn parquet_metadata(path: &Path) -> Result<HashMap<String, String>> {
  let metadata = ArrowReaderMetadata::load(
    &File::open(path).map_err(|source| FixtureError::Io {
      operation: format!("open {}", path.display()),
      source,
    })?,
    ArrowReaderOptions::new(),
  )
  .map_err(|source| FixtureError::Parquet {
    operation: format!("read metadata from {}", path.display()),
    source,
  })?;
  Ok(
    metadata
      .metadata()
      .file_metadata()
      .key_value_metadata()
      .map_or(&[][..], |items| items.as_slice())
      .iter()
      .filter_map(|entry| {
        entry
          .value
          .as_ref()
          .map(|value| (entry.key.clone(), value.clone()))
      })
      .collect(),
  )
}

fn read_batches(path: &Path) -> Result<Vec<RecordBatch>> {
  ParquetRecordBatchReaderBuilder::try_new(File::open(path).map_err(|source| FixtureError::Io {
    operation: format!("open {}", path.display()),
    source,
  })?)
  .map_err(|source| FixtureError::Parquet {
    operation: format!("create reader for {}", path.display()),
    source,
  })?
  .build()
  .map_err(|source| FixtureError::Parquet {
    operation: "build Parquet record batch reader".to_string(),
    source,
  })?
  .collect::<std::result::Result<Vec<_>, arrow_schema::ArrowError>>()
  .map_err(|source| FixtureError::Arrow {
    operation: format!("read record batches from {}", path.display()),
    source,
  })
}

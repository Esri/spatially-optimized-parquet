use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow_array::{ArrayRef, BinaryArray, Float64Array, Int64Array, RecordBatch, StringArray};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use gdal::spatial_ref::SpatialRef;
use parquet::arrow::arrow_writer::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::metadata::KeyValue;
use parquet::file::properties::WriterProperties;
use tempfile::TempDir;
use wkb::writer::{WriteOptions, write_geometry};

pub mod suite;

const BATCH_ROWS: usize = 8_192;
const FLOAT_COLUMN_COUNT: usize = 45;
const STRING_COLUMN_COUNT: usize = 5;
const STRING_COLUMN_NAMES: [&str; STRING_COLUMN_COUNT] =
  ["name", "description", "source_url", "owner", "external_ref"];
const WIDE_FLOAT_COLUMN_COUNT: usize = 185;
const WIDE_STRING_COLUMN_COUNT: usize = 21;
const RANDOM_SEED: u64 = 0x5A17_1A1C_D15C_0DE5;

#[derive(Clone, Copy)]
pub enum FixtureKind {
  Point,
  Polygon,
}

impl FixtureKind {
  fn geometry_type(self) -> &'static str {
    match self {
      Self::Point => "Point",
      Self::Polygon => "Polygon",
    }
  }

  fn sample_geometry(self) -> Vec<u8> {
    geometry_for_row(self, 0)
  }
}

pub struct BenchmarkFixture {
  pub path: PathBuf,
  pub file_bytes: u64,
  pub row_count: usize,
}

pub struct BenchmarkFixtureSet {
  _temp: TempDir,
  pub output_root: PathBuf,
  pub point: BenchmarkFixture,
  pub polygon: BenchmarkFixture,
  pub wide_polygon: BenchmarkFixture,
}

impl BenchmarkFixtureSet {
  pub fn build(target_mib: u64) -> Self {
    let temp = tempfile::tempdir().expect("create benchmark fixture directory");
    let target_bytes = target_mib * 1024 * 1024;
    let point = write_fixture(
      temp.path(),
      FixtureKind::Point,
      target_bytes,
      FLOAT_COLUMN_COUNT,
      STRING_COLUMN_COUNT,
      "point",
    );
    let polygon = write_fixture(
      temp.path(),
      FixtureKind::Polygon,
      target_bytes,
      FLOAT_COLUMN_COUNT,
      STRING_COLUMN_COUNT,
      "polygon",
    );
    let wide_polygon = write_fixture(
      temp.path(),
      FixtureKind::Polygon,
      target_bytes,
      WIDE_FLOAT_COLUMN_COUNT,
      WIDE_STRING_COLUMN_COUNT,
      "polygon-wide-4x",
    );
    let output_root = temp.path().join("output");
    fs::create_dir(&output_root).expect("create benchmark output directory");
    Self {
      _temp: temp,
      output_root,
      point,
      polygon,
      wide_polygon,
    }
  }
}

fn write_fixture(
  root: &Path,
  kind: FixtureKind,
  target_bytes: u64,
  float_column_count: usize,
  string_column_count: usize,
  fixture_name: &str,
) -> BenchmarkFixture {
  let path = root.join(format!("{fixture_name}.parquet"));
  let schema = fixture_schema(float_column_count, string_column_count);
  let sample_string_bytes = (0..string_column_count)
    .map(|column| string_attribute(0, column).len())
    .sum::<usize>();
  let sample_row_bytes =
    kind.sample_geometry().len() + sample_string_bytes + 16 + float_column_count * size_of::<f64>();
  let row_count = (target_bytes as usize / sample_row_bytes).max(BATCH_ROWS);
  let properties = WriterProperties::builder()
    .set_compression(Compression::UNCOMPRESSED)
    .set_dictionary_enabled(false)
    .set_max_row_group_size(128 * 1024)
    .build();
  let mut writer = ArrowWriter::try_new(
    File::create(&path).expect("create benchmark fixture"),
    Arc::clone(&schema),
    Some(properties),
  )
  .expect("create benchmark parquet writer");

  for batch_start in (0..row_count).step_by(BATCH_ROWS) {
    let batch_rows = BATCH_ROWS.min(row_count - batch_start);
    writer
      .write(&fixture_batch(
        Arc::clone(&schema),
        kind,
        batch_start,
        batch_rows,
        float_column_count,
        string_column_count,
      ))
      .expect("write benchmark fixture batch");
  }
  writer.append_key_value_metadata(geoparquet_metadata(kind));
  writer.close().expect("close benchmark fixture");

  BenchmarkFixture {
    file_bytes: fs::metadata(&path)
      .expect("read benchmark fixture metadata")
      .len(),
    path,
    row_count,
  }
}

fn fixture_schema(float_column_count: usize, string_column_count: usize) -> SchemaRef {
  let mut fields = vec![Field::new("id", DataType::Int64, false)];
  fields.extend(
    (0..float_column_count)
      .map(|column| Field::new(format!("value_{column:02}"), DataType::Float64, false)),
  );
  fields.extend(
    (0..string_column_count)
      .map(|column| Field::new(string_column_name(column), DataType::Utf8, false)),
  );
  fields.push(Field::new("geometry", DataType::Binary, false));
  Arc::new(Schema::new(fields))
}

fn fixture_batch(
  schema: SchemaRef,
  kind: FixtureKind,
  batch_start: usize,
  row_count: usize,
  float_column_count: usize,
  string_column_count: usize,
) -> RecordBatch {
  let ids = (batch_start..batch_start + row_count)
    .map(|row| row as i64)
    .collect::<Vec<_>>();
  let geometries = (batch_start..batch_start + row_count)
    .map(|row| geometry_for_row(kind, row))
    .collect::<Vec<_>>();
  let geometry_refs = geometries
    .iter()
    .map(|geometry| geometry.as_slice())
    .collect::<Vec<_>>();

  let mut columns: Vec<ArrayRef> = vec![Arc::new(Int64Array::from(ids))];
  columns.extend((0..float_column_count).map(|column| {
    Arc::new(Float64Array::from_iter_values(
      (batch_start..batch_start + row_count)
        .map(|row| deterministic_float(row as u64, column as u64)),
    )) as ArrayRef
  }));
  columns.extend((0..string_column_count).map(|column| {
    Arc::new(StringArray::from_iter_values(
      (batch_start..batch_start + row_count).map(|row| string_attribute(row as u64, column)),
    )) as ArrayRef
  }));
  columns.push(Arc::new(BinaryArray::from(geometry_refs)));

  RecordBatch::try_new(schema, columns).expect("construct benchmark fixture batch")
}

fn string_column_name(column: usize) -> String {
  STRING_COLUMN_NAMES
    .get(column)
    .map(|name| (*name).to_string())
    .unwrap_or_else(|| format!("text_{column:03}"))
}

fn deterministic_float(row: u64, column: u64) -> f64 {
  let value = deterministic_u64(row, column);
  let mantissa = value >> 11;
  mantissa as f64 * (1.0 / ((1_u64 << 53) as f64))
}

fn deterministic_u64(row: u64, column: u64) -> u64 {
  let mut value = RANDOM_SEED
    ^ row.wrapping_mul(0x9E37_79B9_7F4A_7C15)
    ^ column.wrapping_mul(0xBF58_476D_1CE4_E5B9);
  value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
  value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
  value ^ (value >> 31)
}

fn string_attribute(row: u64, column: usize) -> String {
  match column {
    0 => format!("Feature {}", random_token(row, column as u64, 8, 24)),
    1 => format!(
      "Deterministic benchmark description {}",
      random_token(row, column as u64, 48, 256)
    ),
    2 => format!(
      "https://example.test/features/{}/{}",
      random_token(row, column as u64, 8, 24),
      random_token(row, column as u64 + 11, 16, 64)
    ),
    3 => format!("owner-{}", random_token(row, column as u64, 6, 32)),
    4 => format!("ref_{}", random_token(row, column as u64, 12, 48)),
    _ => format!(
      "attribute-{column}-{}",
      random_token(row, column as u64, 16, 96)
    ),
  }
}

fn random_token(row: u64, column: u64, min_length: usize, max_length: usize) -> String {
  const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
  let mut state = deterministic_u64(row, column);
  let length = min_length + state as usize % (max_length - min_length + 1);
  let mut output = String::with_capacity(length);
  for character_index in 0..length {
    state = deterministic_u64(state, character_index as u64 + column);
    output.push(ALPHABET[state as usize % ALPHABET.len()] as char);
  }
  output
}

fn geometry_for_row(kind: FixtureKind, row: usize) -> Vec<u8> {
  let x = -179.0 + ((row.wrapping_mul(37) % 358_000) as f64 / 1_000.0);
  let y = -89.0 + ((row.wrapping_mul(53) % 178_000) as f64 / 1_000.0);
  let geometry = match kind {
    FixtureKind::Point => geo::Geometry::Point(geo::Point::new(x, y)),
    FixtureKind::Polygon => {
      let radius = 0.005 + (row % 16) as f64 * 0.0005;
      let mut coordinates = (0..64)
        .map(|vertex| {
          let angle = std::f64::consts::TAU * vertex as f64 / 64.0;
          (x + radius * angle.cos(), y + radius * angle.sin())
        })
        .collect::<Vec<_>>();
      coordinates.push(coordinates[0]);
      geo::Geometry::Polygon(geo::Polygon::new(
        geo::LineString::from(coordinates),
        vec![],
      ))
    }
  };
  let mut buffer = Vec::new();
  write_geometry(&mut buffer, &geometry, &WriteOptions::default())
    .expect("encode benchmark geometry");
  buffer
}

fn geoparquet_metadata(kind: FixtureKind) -> KeyValue {
  let projjson = SpatialRef::from_epsg(4326)
    .expect("load EPSG:4326")
    .to_projjson()
    .expect("encode EPSG:4326 PROJJSON");
  let projjson: serde_json::Value =
    serde_json::from_str(&projjson).expect("parse EPSG:4326 PROJJSON");
  let value = serde_json::json!({
    "version": "1.1.0",
    "primary_column": "geometry",
    "columns": {
      "geometry": {
        "encoding": "WKB",
        "geometry_types": [kind.geometry_type()],
        "bbox": [-180.0, -90.0, 180.0, 90.0],
        "crs": projjson
      }
    }
  });
  KeyValue::new("geo".to_string(), Some(value.to_string()))
}

pub fn remove_output(path: &Path) {
  if path.is_dir() {
    fs::remove_dir_all(path).expect("remove benchmark output directory");
  } else if path.exists() {
    fs::remove_file(path).expect("remove benchmark output file");
  }
}

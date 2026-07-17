mod common;

use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use arrow_array::{
  Array, BinaryArray, Float64Array, Int32Array, Int64Array, ListArray, RecordBatch, StringArray,
  StructArray, UInt64Array,
};
use arrow_schema::{DataType, Field, Schema};
use gdal_sys::OGRwkbGeometryType;
use spatial::{
  InputOptions, MultiscaleEncoding, OutputMode, OutputOptions, RowRange, SpatialPipelineOptions,
  SpatialPipelineResult, ValidationRule, WriteProgress, run, validate,
};
use tempfile::TempDir;
use tokio::runtime::Runtime;

use common::assertion::{
  assert_close, assert_covering_metadata, assert_json_extent, binary_value, string_value,
  struct_f64_value,
};
use common::fixture::{wkb_dimensional_point, wkb_dimensional_polygon, wkb_point, wkb_polygon};
use common::geometry::{point_from_wkb_xy, polygon_extent_from_wkb, transform_point_between_epsg};
use common::gpkg::{GpkgFeature, GpkgLayer, write_gpkg};
use common::parquet::{
  geoparquet_kv, geoparquet_kv_with_epsg, kv_map, reader_metadata, scan_parquet, write_parquet,
};

fn runtime() -> Runtime {
  Runtime::new().unwrap()
}

fn run_optimized(
  input: &Path,
  output: &Path,
  row_range: RowRange,
  layer: Option<String>,
  geometry_column: Option<String>,
  input_wkid: Option<u32>,
  covering: bool,
) -> Result<SpatialPipelineResult> {
  run_optimized_with_stripping(
    input,
    output,
    row_range,
    layer,
    geometry_column,
    input_wkid,
    covering,
    false,
    false,
  )
}

fn run_optimized_native(input: &Path, output: &Path) -> Result<SpatialPipelineResult> {
  run_optimized_multiscale(input, output, MultiscaleEncoding::QuantizedNative)
}

fn run_optimized_multiscale(
  input: &Path,
  output: &Path,
  encoding: MultiscaleEncoding,
) -> Result<SpatialPipelineResult> {
  runtime().block_on(run(SpatialPipelineOptions::new(
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
    )
    .with_multiscale_encoding(encoding),
  )))
}

#[allow(clippy::too_many_arguments)]
fn run_optimized_with_stripping(
  input: &Path,
  output: &Path,
  row_range: RowRange,
  layer: Option<String>,
  geometry_column: Option<String>,
  input_wkid: Option<u32>,
  covering: bool,
  strip_z: bool,
  strip_m: bool,
) -> Result<SpatialPipelineResult> {
  runtime().block_on(run(SpatialPipelineOptions::new(
    InputOptions::new(
      input.to_string_lossy(),
      None,
      row_range,
      layer,
      geometry_column,
      input_wkid,
    ),
    OutputOptions::new(
      output,
      OutputMode::OptimizedGeoParquet,
      None,
      None,
      4326,
      covering,
      true,
    )
    .with_stripped_dimensions(strip_z, strip_m),
  )))
}

#[test]
fn optimized_output_sorts_points_and_writes_metadata() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points.parquet");
  let output = temp.path().join("points-optimized.parquet");
  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let point_late = wkb_point(8.0, 8.0);
  let point_early = wkb_point(1.0, 1.0);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(vec!["late", "early"])),
      Arc::new(BinaryArray::from(vec![
        Some(point_late.as_slice()),
        Some(point_early.as_slice()),
      ])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Point"])],
  );

  let progress = Arc::new(Mutex::new(Vec::new()));
  let reported = Arc::clone(&progress);
  let result = runtime()
    .block_on(run(
      SpatialPipelineOptions::new(
        InputOptions::new(
          input.to_string_lossy(),
          None,
          RowRange::default(),
          None,
          None,
          None,
        ),
        OutputOptions::new(
          &output,
          OutputMode::OptimizedGeoParquet,
          None,
          None,
          4326,
          false,
          true,
        ),
      )
      .with_write_reporter(move |update: WriteProgress| {
        reported.lock().unwrap().push(update);
      }),
    ))
    .unwrap();

  assert_eq!(result.rows_expected(), 2);
  assert_eq!(result.rows_written(), 2);
  let progress = progress.lock().unwrap();
  assert_eq!(progress.last().map(|update| update.rows_written()), Some(2));
  assert_eq!(progress.last().map(|update| update.total_rows()), Some(2));
  let explicit_report = validate(&output).unwrap();
  assert!(!explicit_report.has_errors());
  assert!(
    explicit_report
      .findings()
      .iter()
      .any(|finding| finding.rule() == ValidationRule::WriterMetadata)
  );

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let output_schema = Arc::new(dataframe.schema().as_arrow().clone());
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let batch = &batches[0];
  let geodisplay = output_schema.field_with_name("geodisplay").unwrap();
  assert!(geodisplay.is_nullable());
  let DataType::Struct(fields) = geodisplay.data_type() else {
    panic!("geodisplay must be a struct");
  };
  assert!(!fields.find("zCode").unwrap().1.is_nullable());
  assert!(!fields.find("x").unwrap().1.is_nullable());
  assert!(!fields.find("y").unwrap().1.is_nullable());
  assert_eq!(
    string_value(batch.column_by_name("name").unwrap().as_ref(), 0),
    "early"
  );
  assert_eq!(
    binary_value(batch.column_by_name("geometry").unwrap().as_ref(), 0),
    point_early
  );

  let metadata = kv_map(&output);
  assert!(metadata.contains_key("geo"));
  let geo: serde_json::Value = serde_json::from_str(metadata.get("geo").unwrap()).unwrap();
  let geodisplay: serde_json::Value =
    serde_json::from_str(metadata.get("geodisplay").unwrap()).unwrap();
  assert_eq!(geodisplay["parentColumn"], "geodisplay");
  assert_eq!(geodisplay["index"]["type"], "z");
  assert_eq!(geodisplay["index"]["version"], "0.1");
  assert_eq!(geodisplay["index"]["geometryType"], "point");
  assert_eq!(geodisplay["index"]["xColumn"], "x");
  assert_eq!(geodisplay["index"]["yColumn"], "y");
  assert_eq!(geodisplay["index"]["wkid"], 4326);
  assert!(geodisplay["index"].get("wkt").is_none());
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["authority"], "EPSG");
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["code"], 4326);
}

#[test]
fn optimized_output_uses_null_geodisplay_for_null_points() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("nullable-points.parquet");
  let output = temp.path().join("nullable-points-optimized.parquet");
  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let point = wkb_point(1.0, 2.0);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(vec!["missing", "point"])),
      Arc::new(BinaryArray::from(vec![None, Some(point.as_slice())])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Point"])],
  );

  run_optimized(
    &input,
    &output,
    RowRange::default(),
    None,
    None,
    None,
    false,
  )
  .unwrap();

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let output_schema = dataframe.schema().as_arrow().clone();
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let (batch, missing_index) = batches
    .iter()
    .find_map(|batch| {
      let names = batch.column_by_name("name").unwrap();
      (0..batch.num_rows())
        .find(|index| string_value(names.as_ref(), *index) == "missing")
        .map(|index| (batch, index))
    })
    .unwrap();
  let geodisplay_field = output_schema.field_with_name("geodisplay").unwrap();
  let DataType::Struct(fields) = geodisplay_field.data_type() else {
    panic!("geodisplay must be a struct");
  };
  let geodisplay = batch
    .column_by_name("geodisplay")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();

  assert!(geodisplay_field.is_nullable());
  assert!(!fields.find("zCode").unwrap().1.is_nullable());
  assert!(!fields.find("x").unwrap().1.is_nullable());
  assert!(!fields.find("y").unwrap().1.is_nullable());
  assert!(geodisplay.is_null(missing_index));
}

#[test]
fn optimized_output_keeps_point_z_and_m_in_wkb_columns_and_metadata() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points-zm.parquet");
  let output = temp.path().join("points-zm-optimized.parquet");
  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let point = wkb_dimensional_point(1.0, 2.0, Some(30.0), Some(40.0));
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(vec!["dimensional"])),
      Arc::new(BinaryArray::from(vec![Some(point.as_slice())])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Point ZM"])],
  );

  run_optimized(
    &input,
    &output,
    RowRange::default(),
    None,
    None,
    None,
    false,
  )
  .unwrap();
  assert!(!validate(&output).unwrap().has_errors());

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let batch = &batches[0];
  assert_eq!(
    binary_value(batch.column_by_name("geometry").unwrap().as_ref(), 0),
    point
  );
  let geodisplay = batch
    .column_by_name("geodisplay")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  assert_eq!(struct_f64_value(geodisplay, "x", 0), 1.0);
  assert_eq!(struct_f64_value(geodisplay, "y", 0), 2.0);
  assert_eq!(struct_f64_value(geodisplay, "z", 0), 30.0);
  assert_eq!(struct_f64_value(geodisplay, "m", 0), 40.0);

  let metadata = kv_map(&output);
  let geo: serde_json::Value = serde_json::from_str(metadata.get("geo").unwrap()).unwrap();
  let geodisplay_metadata: serde_json::Value =
    serde_json::from_str(metadata.get("geodisplay").unwrap()).unwrap();
  assert_eq!(geo["columns"]["geometry"]["geometry_types"][0], "Point ZM");
  assert_eq!(geodisplay_metadata["index"]["hasZ"], true);
  assert_eq!(geodisplay_metadata["index"]["hasM"], true);
  assert_eq!(geodisplay_metadata["index"]["zColumn"], "z");
  assert_eq!(geodisplay_metadata["index"]["mColumn"], "m");
}

#[test]
fn optimized_output_supports_point_xyz_and_point_xym() {
  let temp = TempDir::new().unwrap();
  for (suffix, geometry_type, z, m) in [
    ("xyz", "Point Z", Some(30.0), None),
    ("xym", "Point M", None, Some(40.0)),
  ] {
    let input = temp.path().join(format!("point-{suffix}.parquet"));
    let output = temp
      .path()
      .join(format!("point-{suffix}-optimized.parquet"));
    let schema = Arc::new(Schema::new(vec![Field::new(
      "geometry",
      DataType::Binary,
      false,
    )]));
    let point = wkb_dimensional_point(1.0, 2.0, z, m);
    let batch = RecordBatch::try_new(
      schema.clone(),
      vec![Arc::new(BinaryArray::from(vec![Some(point.as_slice())]))],
    )
    .unwrap();
    write_parquet(
      &input,
      &schema,
      &[batch],
      parquet::basic::Compression::SNAPPY,
      &[geoparquet_kv("geometry", &[geometry_type])],
    );

    run_optimized(
      &input,
      &output,
      RowRange::default(),
      None,
      None,
      None,
      false,
    )
    .unwrap();
    assert!(!validate(&output).unwrap().has_errors());

    let dataframe = runtime()
      .block_on(scan_parquet(output.to_str().unwrap()))
      .unwrap();
    let batches = runtime().block_on(dataframe.collect()).unwrap();
    let geodisplay = batches[0]
      .column_by_name("geodisplay")
      .unwrap()
      .as_any()
      .downcast_ref::<StructArray>()
      .unwrap();
    assert_eq!(geodisplay.column_by_name("z").is_some(), z.is_some());
    assert_eq!(geodisplay.column_by_name("m").is_some(), m.is_some());
    if let Some(z) = z {
      assert_eq!(struct_f64_value(geodisplay, "z", 0), z);
    }
    if let Some(m) = m {
      assert_eq!(struct_f64_value(geodisplay, "m", 0), m);
    }
  }
}

#[test]
fn optimized_output_strips_z_and_m_independently() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("point-zm.parquet");
  let schema = Arc::new(Schema::new(vec![Field::new(
    "geometry",
    DataType::Binary,
    false,
  )]));
  let point = wkb_dimensional_point(1.0, 2.0, Some(30.0), Some(40.0));
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![Arc::new(BinaryArray::from(vec![Some(point.as_slice())]))],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Point ZM"])],
  );

  for (name, strip_z, strip_m, expected_type, expected_geometry_type) in [
    ("strip-z", true, false, 2001_u32, "Point M"),
    ("strip-m", false, true, 1001_u32, "Point Z"),
    ("strip-zm", true, true, 1_u32, "Point"),
  ] {
    let output = temp.path().join(format!("{name}.parquet"));
    run_optimized_with_stripping(
      &input,
      &output,
      RowRange::default(),
      None,
      None,
      None,
      false,
      strip_z,
      strip_m,
    )
    .unwrap();
    assert!(!validate(&output).unwrap().has_errors());

    let dataframe = runtime()
      .block_on(scan_parquet(output.to_str().unwrap()))
      .unwrap();
    let batches = runtime().block_on(dataframe.collect()).unwrap();
    let geometry = binary_value(batches[0].column_by_name("geometry").unwrap().as_ref(), 0);
    assert_eq!(
      u32::from_le_bytes(geometry[1..5].try_into().unwrap()),
      expected_type
    );
    let geodisplay = batches[0]
      .column_by_name("geodisplay")
      .unwrap()
      .as_any()
      .downcast_ref::<StructArray>()
      .unwrap();
    assert_eq!(geodisplay.column_by_name("z").is_some(), !strip_z);
    assert_eq!(geodisplay.column_by_name("m").is_some(), !strip_m);

    let metadata = kv_map(&output);
    let geo: serde_json::Value = serde_json::from_str(metadata.get("geo").unwrap()).unwrap();
    let geodisplay_metadata: serde_json::Value =
      serde_json::from_str(metadata.get("geodisplay").unwrap()).unwrap();
    assert_eq!(
      geo["columns"]["geometry"]["geometry_types"][0],
      expected_geometry_type
    );
    assert_eq!(geodisplay_metadata["index"]["hasZ"], !strip_z);
    assert_eq!(geodisplay_metadata["index"]["hasM"], !strip_m);
  }
}

#[test]
fn optimized_point_output_rejects_wkb_dimensions_that_disagree_with_metadata() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("point-mismatch.parquet");
  let output = temp.path().join("point-mismatch-output.parquet");
  let schema = Arc::new(Schema::new(vec![Field::new(
    "geometry",
    DataType::Binary,
    false,
  )]));
  let geometry = wkb_point(1.0, 2.0);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![Arc::new(BinaryArray::from(vec![Some(geometry.as_slice())]))],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Point Z"])],
  );

  let error = run_optimized(
    &input,
    &output,
    RowRange::default(),
    None,
    None,
    None,
    false,
  )
  .unwrap_err();
  assert!(
    error
      .to_string()
      .contains("WKB dimensions do not match GeoParquet metadata")
  );
}

#[test]
fn optimized_complex_geometry_output_zero_fills_dimensions_that_disagree_with_metadata() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("polygon-mismatch.parquet");
  let output = temp.path().join("polygon-mismatch-output.parquet");
  let schema = Arc::new(Schema::new(vec![Field::new(
    "geometry",
    DataType::Binary,
    false,
  )]));
  let geometry = wkb_polygon(&[(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 0.0)]);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![Arc::new(BinaryArray::from(vec![Some(geometry.as_slice())]))],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Polygon M"])],
  );

  let result = run_optimized(
    &input,
    &output,
    RowRange::default(),
    None,
    None,
    None,
    false,
  )
  .unwrap();

  assert_eq!(result.rows_written(), 1);
  assert_eq!(result.warnings().len(), 1);
  assert!(result.warnings()[0].contains("encoding missing Z/M values as 0"));
}

#[test]
fn optimized_output_writes_dimensional_polygon_pbf_with_absolute_z_and_m() {
  use prost::Message;

  #[derive(Clone, PartialEq, Message)]
  struct PbfGeometry {
    #[prost(uint32, repeated, tag = "2")]
    lengths: Vec<u32>,
    #[prost(sint64, repeated, tag = "3")]
    coords: Vec<i64>,
  }

  let temp = TempDir::new().unwrap();
  let input = temp.path().join("polygon-zm.parquet");
  let output = temp.path().join("polygon-zm-optimized.parquet");
  let schema = Arc::new(Schema::new(vec![Field::new(
    "geometry",
    DataType::Binary,
    false,
  )]));
  let polygon = wkb_dimensional_polygon(&[
    (0.0, 0.0, Some(10.0), Some(100.0)),
    (4.0, 0.0, Some(20.0), Some(200.0)),
    (4.0, 4.0, Some(30.0), Some(300.0)),
    (0.0, 4.0, Some(40.0), Some(400.0)),
    (0.0, 0.0, Some(10.0), Some(100.0)),
  ]);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![Arc::new(BinaryArray::from(vec![Some(polygon.as_slice())]))],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Polygon ZM"])],
  );

  run_optimized(
    &input,
    &output,
    RowRange::default(),
    None,
    None,
    None,
    false,
  )
  .unwrap();
  assert!(!validate(&output).unwrap().has_errors());

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let geodisplay = batches[0]
    .column_by_name("geodisplay")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  let level_16 = binary_value(geodisplay.column_by_name("level_16").unwrap().as_ref(), 0);
  let decoded = PbfGeometry::decode(level_16.as_slice()).unwrap();
  assert_eq!(decoded.lengths, vec![5]);
  assert_eq!(decoded.coords.len(), 20);
  assert_eq!(
    decoded
      .coords
      .chunks_exact(4)
      .map(|coordinate| (coordinate[2], coordinate[3]))
      .collect::<Vec<_>>(),
    vec![(10, 100), (40, 400), (30, 300), (20, 200), (10, 100)]
  );

  let metadata = kv_map(&output);
  let geodisplay_metadata: serde_json::Value =
    serde_json::from_str(metadata.get("geodisplay").unwrap()).unwrap();
  assert_eq!(geodisplay_metadata["index"]["hasZ"], true);
  assert_eq!(geodisplay_metadata["index"]["hasM"], true);
  assert_eq!(
    geodisplay_metadata["index"]["levels"][0]["transform"]["scale"][2],
    1.0
  );
  assert_eq!(
    geodisplay_metadata["index"]["levels"][0]["transform"]["scale"][3],
    1.0
  );
}

#[test]
fn optimized_output_strips_polygon_pbf_dimensions() {
  use prost::Message;

  #[derive(Clone, PartialEq, Message)]
  struct PbfGeometry {
    #[prost(uint32, repeated, tag = "2")]
    lengths: Vec<u32>,
    #[prost(sint64, repeated, tag = "3")]
    coords: Vec<i64>,
  }

  let temp = TempDir::new().unwrap();
  let input = temp.path().join("polygon-zm.parquet");
  let schema = Arc::new(Schema::new(vec![Field::new(
    "geometry",
    DataType::Binary,
    false,
  )]));
  let polygon = wkb_dimensional_polygon(&[
    (0.0, 0.0, Some(10.0), Some(100.0)),
    (4.0, 0.0, Some(20.0), Some(200.0)),
    (4.0, 4.0, Some(30.0), Some(300.0)),
    (0.0, 0.0, Some(10.0), Some(100.0)),
  ]);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![Arc::new(BinaryArray::from(vec![Some(polygon.as_slice())]))],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Polygon ZM"])],
  );

  for (name, strip_z, strip_m, expected_components) in [
    ("strip-z", true, false, vec![100_i64, 300, 200, 100]),
    ("strip-m", false, true, vec![10_i64, 30, 20, 10]),
  ] {
    let output = temp.path().join(format!("{name}.parquet"));
    run_optimized_with_stripping(
      &input,
      &output,
      RowRange::default(),
      None,
      None,
      None,
      false,
      strip_z,
      strip_m,
    )
    .unwrap();
    assert!(!validate(&output).unwrap().has_errors());

    let dataframe = runtime()
      .block_on(scan_parquet(output.to_str().unwrap()))
      .unwrap();
    let batches = runtime().block_on(dataframe.collect()).unwrap();
    let geodisplay = batches[0]
      .column_by_name("geodisplay")
      .unwrap()
      .as_any()
      .downcast_ref::<StructArray>()
      .unwrap();
    let payload = binary_value(geodisplay.column_by_name("level_16").unwrap().as_ref(), 0);
    let decoded = PbfGeometry::decode(payload.as_slice()).unwrap();
    assert_eq!(decoded.coords.len(), 12);
    assert_eq!(
      decoded
        .coords
        .chunks_exact(3)
        .map(|coordinate| coordinate[2])
        .collect::<Vec<_>>(),
      expected_components
    );
  }
}

#[test]
fn optimized_output_writes_covering_bbox_for_reprojected_points() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points-3857.parquet");
  let output = temp.path().join("points-covering.parquet");
  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let (x, y) = transform_point_between_epsg(1.0, 1.0, 4326, 3857);
  let projected_point = wkb_point(x, y);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(vec!["projected"])),
      Arc::new(BinaryArray::from(vec![Some(projected_point.as_slice())])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv_with_epsg("geometry", &["Point"], 3857)],
  );

  run_optimized(&input, &output, RowRange::default(), None, None, None, true).unwrap();
  assert!(!validate(&output).unwrap().has_errors());

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let output_schema = Arc::new(dataframe.schema().as_arrow().clone());
  assert!(output_schema.index_of("bbox").is_ok());
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let bbox = batches[0]
    .column_by_name("bbox")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  for field in ["xmin", "ymin", "xmax", "ymax"] {
    assert_close(struct_f64_value(bbox, field, 0), 1.0);
  }

  let metadata = kv_map(&output);
  let geo: serde_json::Value = serde_json::from_str(metadata.get("geo").unwrap()).unwrap();
  assert_covering_metadata(&geo);
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["code"], 4326);
}

#[test]
fn optimized_output_reprojects_point_xy_and_preserves_point_z_and_m() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("point-zm-3857.parquet");
  let output = temp.path().join("point-zm-4326.parquet");
  let schema = Arc::new(Schema::new(vec![Field::new(
    "geometry",
    DataType::Binary,
    false,
  )]));
  let (x, y) = transform_point_between_epsg(1.0, 2.0, 4326, 3857);
  let point = wkb_dimensional_point(x, y, Some(30.0), Some(40.0));
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![Arc::new(BinaryArray::from(vec![Some(point.as_slice())]))],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv_with_epsg("geometry", &["Point ZM"], 3857)],
  );

  run_optimized(
    &input,
    &output,
    RowRange::default(),
    None,
    None,
    None,
    false,
  )
  .unwrap();
  assert!(!validate(&output).unwrap().has_errors());

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let geodisplay = batches[0]
    .column_by_name("geodisplay")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  assert_close(struct_f64_value(geodisplay, "x", 0), 1.0);
  assert_close(struct_f64_value(geodisplay, "y", 0), 2.0);
  assert_eq!(struct_f64_value(geodisplay, "z", 0), 30.0);
  assert_eq!(struct_f64_value(geodisplay, "m", 0), 40.0);
}

#[test]
fn optimized_output_reprojects_selected_geoparquet_rows() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points-3857.parquet");
  let output = temp.path().join("points-optimized.parquet");
  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let ignored_point_xy = transform_point_between_epsg(40.0, 30.0, 4326, 3857);
  let selected_point_xy = transform_point_between_epsg(1.0, 1.0, 4326, 3857);
  let ignored_point = wkb_point(ignored_point_xy.0, ignored_point_xy.1);
  let selected_point = wkb_point(selected_point_xy.0, selected_point_xy.1);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(vec!["ignored", "selected"])),
      Arc::new(BinaryArray::from(vec![
        Some(ignored_point.as_slice()),
        Some(selected_point.as_slice()),
      ])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv_with_epsg("geometry", &["Point"], 3857)],
  );

  run_optimized(
    &input,
    &output,
    RowRange::new(1, Some(1)),
    None,
    None,
    None,
    true,
  )
  .unwrap();

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let batch = &batches[0];
  assert_eq!(
    string_value(batch.column_by_name("name").unwrap().as_ref(), 0),
    "selected"
  );
  let geodisplay = batch
    .column_by_name("geodisplay")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  let x = geodisplay
    .column_by_name("x")
    .unwrap()
    .as_any()
    .downcast_ref::<Float64Array>()
    .unwrap();
  let y = geodisplay
    .column_by_name("y")
    .unwrap()
    .as_any()
    .downcast_ref::<Float64Array>()
    .unwrap();
  assert_close(x.value(0), 1.0);
  assert_close(y.value(0), 1.0);
  let geometry = binary_value(batch.column_by_name("geometry").unwrap().as_ref(), 0);
  let (output_x, output_y) = point_from_wkb_xy(&geometry).unwrap();
  assert_close(output_x, 1.0);
  assert_close(output_y, 1.0);

  let metadata = kv_map(&output);
  let geo: serde_json::Value = serde_json::from_str(metadata.get("geo").unwrap()).unwrap();
  let geodisplay: serde_json::Value =
    serde_json::from_str(metadata.get("geodisplay").unwrap()).unwrap();
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["code"], 4326);
  assert_json_extent(&geo["columns"]["geometry"]["bbox"], [1.0, 1.0, 1.0, 1.0]);
  assert_json_extent(&geodisplay["index"]["fullExtent"], [1.0, 1.0, 1.0, 1.0]);
  assert_eq!(geodisplay["index"]["wkid"], 4326);
  assert!(geodisplay["index"].get("wkt").is_none());
  assert!(!metadata.get("geo").unwrap().contains("3857"));
  assert!(!metadata.get("geodisplay").unwrap().contains("3857"));
}

#[test]
fn optimized_output_writes_complex_geometry_display_struct_and_metadata() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("polygons.parquet");
  let output = temp.path().join("polygons-optimized.parquet");
  let schema = Arc::new(Schema::new(vec![
    Field::new("id", DataType::Int32, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let polygon_late = wkb_polygon(&[(7.0, 7.0), (9.0, 7.0), (9.0, 9.0), (7.0, 7.0)]);
  let polygon_early = wkb_polygon(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 0.0)]);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(Int32Array::from(vec![2, 1])),
      Arc::new(BinaryArray::from(vec![
        Some(polygon_late.as_slice()),
        Some(polygon_early.as_slice()),
      ])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Polygon"])],
  );
  run_optimized(
    &input,
    &output,
    RowRange::default(),
    None,
    None,
    None,
    false,
  )
  .unwrap();
  let validation = validate(&output).unwrap();
  assert!(
    validation
      .findings()
      .iter()
      .all(|finding| finding.rule() != ValidationRule::PbfWinding)
  );

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let output_schema = Arc::new(dataframe.schema().as_arrow().clone());
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let batch = &batches[0];
  let ids = batch
    .column_by_name("id")
    .unwrap()
    .as_any()
    .downcast_ref::<Int32Array>()
    .unwrap();
  let geodisplay_column = batch
    .column_by_name("geodisplay")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  assert!(output_schema.index_of("geodisplay").is_ok());
  assert_eq!(ids.value(0), 1);
  assert!(geodisplay_column.column_by_name("xzCode").is_some());
  assert!(geodisplay_column.column_by_name("bounds").is_none());

  let metadata = kv_map(&output);
  let geo: serde_json::Value = serde_json::from_str(metadata.get("geo").unwrap()).unwrap();
  let geodisplay: serde_json::Value =
    serde_json::from_str(metadata.get("geodisplay").unwrap()).unwrap();
  assert_eq!(geodisplay["parentColumn"], "geodisplay");
  assert_eq!(geodisplay["index"]["type"], "xz");
  assert_eq!(geodisplay["index"]["version"], "0.1");
  assert_eq!(geodisplay["index"]["encoding"], "esriPBF");
  assert_eq!(geodisplay["index"]["wkid"], 4326);
  assert!(geodisplay["index"].get("wkt").is_none());
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["authority"], "EPSG");
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["code"], 4326);
  let levels = geodisplay["index"]["levels"].as_array().unwrap();
  assert_eq!(levels.len(), 9);
  assert_eq!(levels[0]["level"], 0);
  assert_eq!(levels[0]["resolution"], 0.703125);
  assert_eq!(levels[0]["scale"], 295829355.4545656);
  assert_eq!(levels[0]["transform"]["scale"][0], 0.703125);
  assert_eq!(levels[0]["transform"]["scale"][1], 0.703125);
  assert_eq!(levels[0]["transform"]["translate"][0], 0.0);
  assert_eq!(levels[0]["transform"]["translate"][1], 0.0);
  assert_eq!(levels[1]["level"], 2);
  assert_eq!(levels[1]["resolution"], 0.17578125);
  assert_eq!(levels[1]["scale"], 73957338.8636414);
}

#[test]
fn optimized_output_writes_native_quantized_multiscale_geometry() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("polygons.parquet");
  let output = temp.path().join("polygons-native.parquet");
  let schema = Arc::new(Schema::new(vec![
    Field::new("id", DataType::Int32, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let polygon = wkb_polygon(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 0.0)]);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(Int32Array::from(vec![1])),
      Arc::new(BinaryArray::from(vec![Some(polygon.as_slice())])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Polygon"])],
  );

  run_optimized_native(&input, &output).unwrap();
  validate(&output).unwrap().ensure_valid().unwrap();

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let geodisplay = batches[0]
    .column_by_name("geodisplay")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  let geometries = geodisplay
    .column_by_name("level_16")
    .unwrap()
    .as_any()
    .downcast_ref::<ListArray>()
    .unwrap();
  let parts = geometries
    .value(0)
    .as_any()
    .downcast_ref::<ListArray>()
    .unwrap()
    .clone();
  let coordinates = parts
    .value(0)
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap()
    .clone();
  let x = coordinates
    .column_by_name("x")
    .unwrap()
    .as_any()
    .downcast_ref::<Int64Array>()
    .unwrap();
  assert_eq!(x.value(0), 0);
  assert!(x.value(1) > 0);
  assert_eq!(x.value(1), x.value(2));
  assert_eq!(x.value(3), 0);

  let parquet_metadata = reader_metadata(&output);
  let coordinate_column = parquet_metadata
    .metadata()
    .row_group(0)
    .columns()
    .iter()
    .find(|column| {
      column
        .column_descr()
        .path()
        .string()
        .ends_with("level_16.list.element.list.element.x")
    })
    .expect("native x coordinate column");
  let encodings = coordinate_column.encodings().collect::<Vec<_>>();
  assert!(encodings.contains(&parquet::basic::Encoding::DELTA_BINARY_PACKED));
  assert!(!encodings.contains(&parquet::basic::Encoding::RLE_DICTIONARY));
  assert!(!encodings.contains(&parquet::basic::Encoding::PLAIN_DICTIONARY));
}

#[test]
fn optimized_native_output_writes_missing_values_as_nullable_components_zm() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("polygon-missing-zm.parquet");
  let output = temp.path().join("polygon-missing-zm-native.parquet");
  let schema = Arc::new(Schema::new(vec![Field::new(
    "geometry",
    DataType::Binary,
    false,
  )]));
  let polygon = wkb_dimensional_polygon(&[
    (0.0, 0.0, Some(f64::NAN), Some(f64::NAN)),
    (2.0, 0.0, Some(f64::NAN), Some(f64::NAN)),
    (2.0, 2.0, Some(f64::NAN), Some(f64::NAN)),
    (0.0, 0.0, Some(f64::NAN), Some(f64::NAN)),
  ]);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![Arc::new(BinaryArray::from(vec![Some(polygon.as_slice())]))],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Polygon ZM"])],
  );

  let result = run_optimized_native(&input, &output).unwrap();
  validate(&output).unwrap().ensure_valid().unwrap();
  assert!(result.warnings().is_empty());

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let geodisplay = batches[0]
    .column_by_name("geodisplay")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  let geometries = geodisplay
    .column_by_name("level_16")
    .unwrap()
    .as_any()
    .downcast_ref::<ListArray>()
    .unwrap();
  let parts = geometries
    .value(0)
    .as_any()
    .downcast_ref::<ListArray>()
    .unwrap()
    .clone();
  let coordinates = parts
    .value(0)
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap()
    .clone();
  let z = coordinates
    .column_by_name("z")
    .unwrap()
    .as_any()
    .downcast_ref::<Int64Array>()
    .unwrap();
  let m = coordinates
    .column_by_name("m")
    .unwrap()
    .as_any()
    .downcast_ref::<Int64Array>()
    .unwrap();

  assert_eq!(z.null_count(), coordinates.len());
  assert_eq!(m.null_count(), coordinates.len());
  let DataType::Struct(fields) = coordinates.data_type() else {
    panic!("native coordinates must use a struct")
  };
  assert!(fields.find("z").expect("z field").1.is_nullable());
  assert!(fields.find("m").expect("m field").1.is_nullable());
}

#[test]
fn optimized_output_writes_covering_bbox_for_complex_geometry() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("polygons.parquet");
  let output = temp.path().join("polygons-covering.parquet");
  let schema = Arc::new(Schema::new(vec![
    Field::new("id", DataType::Int32, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let polygon = wkb_polygon(&[(0.0, 0.0), (2.0, 0.0), (2.0, 3.0), (0.0, 0.0)]);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(Int32Array::from(vec![1])),
      Arc::new(BinaryArray::from(vec![Some(polygon.as_slice())])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Polygon"])],
  );
  run_optimized(&input, &output, RowRange::default(), None, None, None, true).unwrap();

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let output_schema = Arc::new(dataframe.schema().as_arrow().clone());
  assert!(output_schema.index_of("bbox").is_ok());
  assert!(output_schema.index_of("geodisplay").is_ok());
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let bbox = batches[0]
    .column_by_name("bbox")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  assert_close(struct_f64_value(bbox, "xmin", 0), 0.0);
  assert_close(struct_f64_value(bbox, "ymin", 0), 0.0);
  assert_close(struct_f64_value(bbox, "xmax", 0), 2.0);
  assert_close(struct_f64_value(bbox, "ymax", 0), 3.0);
  let geo: serde_json::Value = serde_json::from_str(kv_map(&output).get("geo").unwrap()).unwrap();
  assert_covering_metadata(&geo);
}

#[test]
fn optimized_output_replaces_existing_display_column() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("polygons-with-geodisplay.parquet");
  let output = temp.path().join("polygons-regenerated.parquet");
  let schema = Arc::new(Schema::new(vec![
    Field::new("id", DataType::Int32, false),
    Field::new("geometry", DataType::Binary, true),
    Field::new("geodisplay", DataType::Utf8, true),
  ]));
  let polygon = wkb_polygon(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 0.0)]);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(Int32Array::from(vec![1])),
      Arc::new(BinaryArray::from(vec![Some(polygon.as_slice())])),
      Arc::new(StringArray::from(vec![Some("stale-geodisplay")])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Polygon"])],
  );
  run_optimized(
    &input,
    &output,
    RowRange::default(),
    None,
    None,
    None,
    false,
  )
  .unwrap();

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let output_schema = Arc::new(dataframe.schema().as_arrow().clone());
  let geodisplay_fields = output_schema
    .fields()
    .iter()
    .filter(|field| field.name() == "geodisplay")
    .count();
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let geodisplay_column = batches[0]
    .column_by_name("geodisplay")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  assert_eq!(geodisplay_fields, 1);
  assert!(geodisplay_column.column_by_name("xzCode").is_some());
  assert!(geodisplay_column.column_by_name("bounds").is_none());
}

#[test]
fn optimized_output_sorts_complex_geometry_rows_across_input_batches() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("polygons-multi-batch.parquet");
  let output = temp.path().join("polygons-multi-batch-optimized.parquet");
  let schema = Arc::new(Schema::new(vec![
    Field::new("id", DataType::Int32, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let polygon_1 = wkb_polygon(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 0.0)]);
  let polygon_2 = wkb_polygon(&[(2.0, 0.0), (3.0, 0.0), (3.0, 1.0), (2.0, 0.0)]);
  let polygon_3 = wkb_polygon(&[(4.0, 0.0), (5.0, 0.0), (5.0, 1.0), (4.0, 0.0)]);
  let polygon_4 = wkb_polygon(&[(8.0, 0.0), (9.0, 0.0), (9.0, 1.0), (8.0, 0.0)]);
  let batch_a = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(Int32Array::from(vec![4, 2])),
      Arc::new(BinaryArray::from(vec![
        Some(polygon_4.as_slice()),
        Some(polygon_2.as_slice()),
      ])),
    ],
  )
  .unwrap();
  let batch_b = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(Int32Array::from(vec![3, 1])),
      Arc::new(BinaryArray::from(vec![
        Some(polygon_3.as_slice()),
        Some(polygon_1.as_slice()),
      ])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch_a, batch_b],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Polygon"])],
  );
  run_optimized(
    &input,
    &output,
    RowRange::default(),
    None,
    None,
    None,
    false,
  )
  .unwrap();

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let xz_codes = batches
    .iter()
    .flat_map(|batch| {
      let geodisplay_column = batch
        .column_by_name("geodisplay")
        .unwrap()
        .as_any()
        .downcast_ref::<StructArray>()
        .unwrap();
      let xz_codes = geodisplay_column
        .column_by_name("xzCode")
        .unwrap()
        .as_any()
        .downcast_ref::<UInt64Array>()
        .unwrap();
      (0..batch.num_rows()).map(move |index| xz_codes.value(index))
    })
    .collect::<Vec<_>>();
  assert!(xz_codes.windows(2).all(|pair| pair[0] <= pair[1]));
}

#[test]
fn optimized_output_applies_requested_row_range() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points.parquet");
  let output = temp.path().join("points-limited.parquet");
  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let point_late = wkb_point(8.0, 8.0);
  let point_early = wkb_point(1.0, 1.0);
  let point_middle = wkb_point(4.0, 4.0);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(vec!["late", "early", "middle"])),
      Arc::new(BinaryArray::from(vec![
        Some(point_late.as_slice()),
        Some(point_early.as_slice()),
        Some(point_middle.as_slice()),
      ])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Point"])],
  );
  run_optimized(
    &input,
    &output,
    RowRange::new(1, Some(1)),
    None,
    None,
    None,
    false,
  )
  .unwrap();

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(), 1);
  let names = batches
    .iter()
    .flat_map(|batch| {
      let column = batch.column_by_name("name").unwrap();
      (0..batch.num_rows()).map(move |index| string_value(column.as_ref(), index))
    })
    .collect::<Vec<_>>();
  assert_eq!(names, vec!["early".to_string()]);
}

#[test]
fn optimized_output_overwrites_unmanaged_bbox_column() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points-with-bbox.parquet");
  let output = temp.path().join("points-covering.parquet");
  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
    Field::new("bbox", DataType::Utf8, true),
  ]));
  let point = wkb_point(1.0, 1.0);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(vec!["point"])),
      Arc::new(BinaryArray::from(vec![Some(point.as_slice())])),
      Arc::new(StringArray::from(vec![Some("existing")])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Point"])],
  );
  run_optimized(&input, &output, RowRange::default(), None, None, None, true).unwrap();

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let bbox = batches[0]
    .column_by_name("bbox")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  assert_close(struct_f64_value(bbox, "xmin", 0), 1.0);
  assert_close(struct_f64_value(bbox, "ymin", 0), 1.0);
}

#[test]
fn optimized_output_scans_geometry_type_when_metadata_is_missing() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points.parquet");
  let output = temp.path().join("points-optimized.parquet");
  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let point_late = wkb_point(8.0, 8.0);
  let point_early = wkb_point(1.0, 1.0);
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(vec!["late", "early"])),
      Arc::new(BinaryArray::from(vec![
        Some(point_late.as_slice()),
        Some(point_early.as_slice()),
      ])),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv_with_epsg("geometry", &[], 4326)],
  );
  run_optimized(
    &input,
    &output,
    RowRange::default(),
    None,
    None,
    None,
    false,
  )
  .unwrap();

  let metadata = kv_map(&output);
  let geo: serde_json::Value = serde_json::from_str(metadata.get("geo").unwrap()).unwrap();
  assert_eq!(
    geo["columns"]["geometry"]["geometry_types"],
    serde_json::json!(["Point"])
  );
}

#[test]
fn optimized_output_accepts_single_layer_geopackage() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points.gpkg");
  let output = temp.path().join("points-optimized.parquet");
  let features = [
    GpkgFeature {
      id: 2,
      name: Some("late"),
      geometry_wkt: "POINT (8 8)",
    },
    GpkgFeature {
      id: 1,
      name: Some("early"),
      geometry_wkt: "POINT (1 1)",
    },
  ];
  write_gpkg(
    &input,
    &[GpkgLayer {
      name: "points",
      geometry_type: OGRwkbGeometryType::wkbPoint,
      epsg: Some(4326),
      features: &features,
    }],
  );
  run_optimized(
    &input,
    &output,
    RowRange::default(),
    None,
    None,
    None,
    false,
  )
  .unwrap();

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let output_schema = Arc::new(dataframe.schema().as_arrow().clone());
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let batch = &batches[0];
  let geodisplay = output_schema.field_with_name("geodisplay").unwrap();
  assert!(matches!(geodisplay.data_type(), DataType::Struct(_)));
  assert_eq!(
    string_value(batch.column_by_name("name").unwrap().as_ref(), 0),
    "early"
  );
  let metadata = kv_map(&output);
  let geo: serde_json::Value = serde_json::from_str(metadata.get("geo").unwrap()).unwrap();
  assert_eq!(geo["primary_column"], "geometry");
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["authority"], "EPSG");
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["code"], 4326);
}

#[test]
fn optimized_output_reprojects_geopackage_polygon() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("polygons-3857.gpkg");
  let output = temp.path().join("polygons-optimized.parquet");
  let ring = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (0.0, 0.0)];
  let ring_3857 = ring
    .into_iter()
    .map(|(x, y)| transform_point_between_epsg(x, y, 4326, 3857))
    .map(|(x, y)| format!("{x} {y}"))
    .collect::<Vec<_>>()
    .join(", ");
  let polygon_wkt = format!("POLYGON (({ring_3857}))");
  let features = [GpkgFeature {
    id: 1,
    name: Some("projected"),
    geometry_wkt: &polygon_wkt,
  }];
  write_gpkg(
    &input,
    &[GpkgLayer {
      name: "polygons",
      geometry_type: OGRwkbGeometryType::wkbPolygon,
      epsg: Some(3857),
      features: &features,
    }],
  );
  run_optimized(&input, &output, RowRange::default(), None, None, None, true).unwrap();

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let batch = &batches[0];
  let geometry = binary_value(batch.column_by_name("geometry").unwrap().as_ref(), 0);
  let extent = polygon_extent_from_wkb(&geometry).unwrap();
  for (actual, expected) in extent.into_iter().zip([0.0, 0.0, 1.0, 1.0]) {
    assert_close(actual, expected);
  }

  let geodisplay_column = batch
    .column_by_name("geodisplay")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  assert!(geodisplay_column.column_by_name("bounds").is_none());
  let level_zero = geodisplay_column.column_by_name("level_0").unwrap();
  assert!(!binary_value(level_zero.as_ref(), 0).is_empty());

  let covering = batch
    .column_by_name("bbox")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  assert_close(struct_f64_value(covering, "xmin", 0), 0.0);
  assert_close(struct_f64_value(covering, "ymin", 0), 0.0);
  assert_close(struct_f64_value(covering, "xmax", 0), 1.0);
  assert_close(struct_f64_value(covering, "ymax", 0), 1.0);

  let metadata = kv_map(&output);
  let geo: serde_json::Value = serde_json::from_str(metadata.get("geo").unwrap()).unwrap();
  let geodisplay: serde_json::Value =
    serde_json::from_str(metadata.get("geodisplay").unwrap()).unwrap();
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["code"], 4326);
  assert_covering_metadata(&geo);
  assert_json_extent(&geo["columns"]["geometry"]["bbox"], [0.0, 0.0, 1.0, 1.0]);
  assert_json_extent(&geodisplay["index"]["fullExtent"], [0.0, 0.0, 1.0, 1.0]);
  assert_eq!(geodisplay["index"]["wkid"], 4326);
  assert!(geodisplay["index"].get("wkt").is_none());
  assert_eq!(geodisplay["index"]["levels"][0]["column"], "level_0");
  assert_eq!(geodisplay["index"]["levels"][0]["resolution"], 0.703125);
  assert_eq!(
    geodisplay["index"]["levels"][0]["transform"]["scale"][0],
    0.703125
  );
  assert!(!metadata.get("geo").unwrap().contains("3857"));
  assert!(!metadata.get("geodisplay").unwrap().contains("3857"));
}

#[test]
fn optimized_output_selects_requested_geopackage_layer() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("multi.gpkg");
  let output = temp.path().join("polygons-optimized.parquet");
  let point_features = [GpkgFeature {
    id: 10,
    name: Some("point"),
    geometry_wkt: "POINT (0 0)",
  }];
  let polygon_features = [
    GpkgFeature {
      id: 2,
      name: Some("late"),
      geometry_wkt: "POLYGON ((7 7, 9 7, 9 9, 7 7))",
    },
    GpkgFeature {
      id: 1,
      name: Some("early"),
      geometry_wkt: "POLYGON ((0 0, 1 0, 1 1, 0 0))",
    },
  ];
  write_gpkg(
    &input,
    &[
      GpkgLayer {
        name: "points",
        geometry_type: OGRwkbGeometryType::wkbPoint,
        epsg: Some(4326),
        features: &point_features,
      },
      GpkgLayer {
        name: "polygons",
        geometry_type: OGRwkbGeometryType::wkbPolygon,
        epsg: Some(4326),
        features: &polygon_features,
      },
    ],
  );
  run_optimized(
    &input,
    &output,
    RowRange::default(),
    Some("polygons".to_string()),
    None,
    None,
    false,
  )
  .unwrap();

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let output_schema = Arc::new(dataframe.schema().as_arrow().clone());
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let batch = &batches[0];
  let ids = batch
    .column_by_name("id")
    .unwrap()
    .as_any()
    .downcast_ref::<Int32Array>()
    .unwrap();
  let geodisplay_column = batch
    .column_by_name("geodisplay")
    .unwrap()
    .as_any()
    .downcast_ref::<StructArray>()
    .unwrap();
  assert!(output_schema.index_of("geodisplay").is_ok());
  assert_eq!(ids.value(0), 1);
  assert!(geodisplay_column.column_by_name("xzCode").is_some());
  assert!(geodisplay_column.column_by_name("bounds").is_none());

  let metadata = kv_map(&output);
  let geo: serde_json::Value = serde_json::from_str(metadata.get("geo").unwrap()).unwrap();
  let geodisplay: serde_json::Value =
    serde_json::from_str(metadata.get("geodisplay").unwrap()).unwrap();
  assert_eq!(geo["primary_column"], "geometry");
  assert_eq!(geodisplay["parentColumn"], "geodisplay");
  assert_eq!(geodisplay["index"]["type"], "xz");
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["authority"], "EPSG");
  assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["code"], 4326);
}

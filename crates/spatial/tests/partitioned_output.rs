mod common;

use std::path::Path;
use std::sync::{Arc, Mutex};

use arrow_array::{
  Array, BinaryArray, Float64Array, RecordBatch, StringArray, StructArray, UInt64Array,
};
use arrow_schema::{DataType, Field, Schema};
use parquet::basic::Compression;
use spatial::{
  InputOptions, MultiscaleEncoding, OutputMode, OutputOptions, Pipeline, RowRange,
  SpatialPipelineOptions, WriteProgress, validate,
};
use tempfile::TempDir;
use tokio::runtime::Runtime;

use common::assertion::{
  assert_close, assert_covering_metadata, assert_json_extent, binary_value, string_value,
};
use common::fixture::{wkb_point, wkb_polygon};
use common::geometry::{point_from_wkb_xy, transform_point_between_epsg};
use common::parquet::{
  geoparquet_kv, geoparquet_kv_with_epsg, kv_map, parquet_files, raw_parquet_schema,
  reader_metadata, scan_parquet, write_parquet,
};

fn runtime() -> Runtime {
  Runtime::new().unwrap()
}

fn write_point_input(path: &Path, coordinates: &[(f64, f64)], names: &[&str], epsg: u32) {
  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let points = coordinates
    .iter()
    .map(|&(x, y)| {
      let projected = if epsg == 4326 {
        (x, y)
      } else {
        transform_point_between_epsg(x, y, 4326, epsg)
      };
      wkb_point(projected.0, projected.1)
    })
    .collect::<Vec<_>>();
  let geometries = points
    .iter()
    .map(|point| Some(point.as_slice()))
    .collect::<Vec<_>>();
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(names.to_vec())),
      Arc::new(BinaryArray::from(geometries)),
    ],
  )
  .unwrap();
  write_parquet(
    path,
    &schema,
    &[batch],
    Compression::SNAPPY,
    &[geoparquet_kv_with_epsg("geometry", &["Point"], epsg)],
  );
}

#[test]
fn partitioned_output_writes_sorted_range_partitions() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points.parquet");
  let output = temp.path().join("out");
  write_point_input(
    &input,
    &[(8.0, 8.0), (1.0, 1.0), (4.0, 4.0)],
    &["late", "early", "middle"],
    3857,
  );

  let progress = Arc::new(Mutex::new(Vec::new()));
  let reported = Arc::clone(&progress);
  let result = runtime()
    .block_on(Pipeline::run(SpatialPipelineOptions {
      input: InputOptions {
        location: input.to_string_lossy().into_owned(),
        ..Default::default()
      },
      output: OutputOptions {
        path: output.clone(),
        mode: OutputMode::Optimized,
        file_count: Some(2),
        overwrite: true,
        ..Default::default()
      },
      write_reporter: Some(Arc::new(move |update: WriteProgress| {
        reported.lock().unwrap().push(update);
      })),
      ..Default::default()
    }))
    .unwrap();

  assert_eq!(result.rows_expected(), 3);
  assert_eq!(result.rows_written(), 3);
  assert!(!validate(&output).unwrap().has_errors());
  let progress = progress.lock().unwrap();
  assert_eq!(progress.last().map(|update| update.rows_written()), Some(3));
  assert_eq!(progress.last().map(|update| update.total_rows()), Some(3));
  assert!(
    progress
      .windows(2)
      .all(|updates| updates[0].rows_written() <= updates[1].rows_written())
  );

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(), 3);
  for batch in &batches {
    let sop_geometry = batch
      .column_by_name("sop_geometry")
      .unwrap()
      .as_any()
      .downcast_ref::<StructArray>()
      .unwrap();
    let x = sop_geometry
      .column_by_name("x")
      .unwrap()
      .as_any()
      .downcast_ref::<Float64Array>()
      .unwrap();
    let y = sop_geometry
      .column_by_name("y")
      .unwrap()
      .as_any()
      .downcast_ref::<Float64Array>()
      .unwrap();
    for row_index in 0..batch.num_rows() {
      let geometry = binary_value(
        batch.column_by_name("geometry").unwrap().as_ref(),
        row_index,
      );
      let (geometry_x, geometry_y) = point_from_wkb_xy(&geometry).unwrap();
      assert_close(geometry_x, x.value(row_index));
      assert_close(geometry_y, y.value(row_index));
      assert!((1.0 - 1.0e-5..=8.0 + 1.0e-5).contains(&geometry_x));
      assert!((1.0 - 1.0e-5..=8.0 + 1.0e-5).contains(&geometry_y));
    }
  }

  let range_directories = std::fs::read_dir(&output)
    .unwrap()
    .map(|entry| entry.unwrap().path())
    .filter(|path| path.is_dir())
    .collect::<Vec<_>>();
  assert_eq!(range_directories.len(), 2);
  let mut lower_bounds = range_directories
    .iter()
    .map(|path| {
      path
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix("z_order=")
        .expect("expected z_order partition directory")
        .parse::<u64>()
        .unwrap()
    })
    .collect::<Vec<_>>();
  lower_bounds.sort_unstable();
  assert!(lower_bounds.windows(2).all(|pair| pair[0] < pair[1]));

  let files = parquet_files(&output);
  assert_eq!(files.len(), 2);
  for file in &files {
    let schema = raw_parquet_schema(file);
    let sop_geometry = schema.field_with_name("sop_geometry").unwrap();
    let DataType::Struct(fields) = sop_geometry.data_type() else {
      panic!("sop_geometry must be a struct");
    };
    assert!(schema.field_with_name("geokey").is_ok());
    assert!(fields.find("x").is_some());
    assert!(fields.find("y").is_some());
    assert!(schema.field_with_name("z_order").is_err());
    let metadata = kv_map(file);
    let geo: serde_json::Value = serde_json::from_str(metadata.get("geo").unwrap()).unwrap();
    let geodisplay: serde_json::Value =
      serde_json::from_str(metadata.get("geodisplay").unwrap()).unwrap();
    assert_eq!(geo["columns"]["geometry"]["crs"]["id"]["code"], 4326);
    assert_json_extent(&geo["columns"]["geometry"]["bbox"], [1.0, 1.0, 8.0, 8.0]);
    assert_eq!(geodisplay["wkid"], 4326);
    assert!(!metadata.get("geo").unwrap().contains("3857"));
    assert!(!metadata.get("geodisplay").unwrap().contains("3857"));
  }

  for range_directory in range_directories {
    let dataframe = runtime()
      .block_on(scan_parquet(range_directory.to_str().unwrap()))
      .unwrap();
    let batches = runtime().block_on(dataframe.collect()).unwrap();
    let z_codes = batches
      .iter()
      .flat_map(|batch| {
        let geokey = batch
          .column_by_name("geokey")
          .unwrap()
          .as_any()
          .downcast_ref::<UInt64Array>()
          .unwrap();
        (0..batch.num_rows()).map(move |index| geokey.value(index))
      })
      .collect::<Vec<_>>();
    assert!(z_codes.windows(2).all(|pair| pair[0] <= pair[1]));
    assert!(std::fs::read_dir(&range_directory).unwrap().any(|entry| {
      entry
        .unwrap()
        .path()
        .extension()
        .is_some_and(|extension| extension == "parquet")
    }));
  }
}

#[test]
fn partitioned_output_writes_native_multiscale_coordinate_leaves() {
  assert_partitioned_multiscale_coordinate_leaves(
    MultiscaleEncoding::NativeQuantized,
    &["level_16.list.element.list.element.x"],
    parquet::basic::Encoding::DELTA_BINARY_PACKED,
  );
}

#[test]
fn partitioned_output_writes_native_float_multiscale_coordinate_leaves() {
  for encoding in [
    MultiscaleEncoding::NativeQuantizedFloat,
    MultiscaleEncoding::Native,
  ] {
    assert_partitioned_multiscale_coordinate_leaves(
      encoding,
      &["level_16.list.element.list.element.x"],
      parquet::basic::Encoding::BYTE_STREAM_SPLIT,
    );
  }
}

fn assert_partitioned_multiscale_coordinate_leaves(
  encoding: MultiscaleEncoding,
  leaf_suffixes: &[&str],
  expected_encoding: parquet::basic::Encoding,
) {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("polygons.parquet");
  let output = temp.path().join("out");
  let schema = Arc::new(Schema::new(vec![
    Field::new("name", DataType::Utf8, false),
    Field::new("geometry", DataType::Binary, true),
  ]));
  let polygons = [
    wkb_polygon(&[(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 0.0)]),
    wkb_polygon(&[(4.0, 4.0), (5.0, 4.0), (5.0, 5.0), (4.0, 4.0)]),
    wkb_polygon(&[(8.0, 8.0), (9.0, 8.0), (9.0, 9.0), (8.0, 8.0)]),
  ];
  let batch = RecordBatch::try_new(
    schema.clone(),
    vec![
      Arc::new(StringArray::from(vec!["early", "middle", "late"])),
      Arc::new(BinaryArray::from(
        polygons
          .iter()
          .map(|polygon| Some(polygon.as_slice()))
          .collect::<Vec<_>>(),
      )),
    ],
  )
  .unwrap();
  write_parquet(
    &input,
    &schema,
    &[batch],
    Compression::SNAPPY,
    &[geoparquet_kv("geometry", &["Polygon"])],
  );

  runtime()
    .block_on(Pipeline::run(SpatialPipelineOptions {
      input: InputOptions {
        location: input.to_string_lossy().into_owned(),
        ..Default::default()
      },
      output: OutputOptions {
        path: output.clone(),
        mode: OutputMode::Optimized,
        file_count: Some(2),
        overwrite: true,
        multiscale_encoding: encoding,
        ..Default::default()
      },
      ..Default::default()
    }))
    .unwrap();

  validate(&output).unwrap().ensure_valid().unwrap();
  let files = parquet_files(&output);
  assert_eq!(files.len(), 2);
  for file in files {
    let parquet_metadata = reader_metadata(&file);
    for leaf_suffix in leaf_suffixes {
      let coordinate_column = parquet_metadata
        .metadata()
        .row_group(0)
        .columns()
        .iter()
        .find(|column| column.column_descr().path().string().ends_with(leaf_suffix))
        .unwrap_or_else(|| panic!("missing coordinate column ending in {leaf_suffix}"));
      let encodings = coordinate_column.encodings().collect::<Vec<_>>();
      assert!(encodings.contains(&expected_encoding));
      assert!(!encodings.contains(&parquet::basic::Encoding::RLE_DICTIONARY));
      assert!(!encodings.contains(&parquet::basic::Encoding::PLAIN_DICTIONARY));
    }
  }
}

#[test]
fn partitioned_output_combines_row_range_covering_and_compression() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points.parquet");
  let output = temp.path().join("out");
  write_point_input(
    &input,
    &[(9.0, 9.0), (1.0, 1.0), (5.0, 5.0), (3.0, 3.0), (7.0, 7.0)],
    &["ignored-start", "one", "five", "three", "ignored-end"],
    4326,
  );

  let result = runtime()
    .block_on(Pipeline::run(SpatialPipelineOptions {
      input: InputOptions {
        location: input.to_string_lossy().into_owned(),
        row_range: RowRange::new(1, Some(3)),
        ..Default::default()
      },
      output: OutputOptions {
        path: output.clone(),
        mode: OutputMode::Optimized,
        file_count: Some(2),
        compression: Some("zstd".to_string()),
        covering: true,
        overwrite: true,
        ..Default::default()
      },
      ..Default::default()
    }))
    .unwrap();
  assert_eq!(result.rows_expected(), 3);
  assert_eq!(result.rows_written(), 3);
  assert!(!validate(&output).unwrap().has_errors());

  let dataframe = runtime()
    .block_on(scan_parquet(output.to_str().unwrap()))
    .unwrap();
  let batches = runtime().block_on(dataframe.collect()).unwrap();
  let mut names = batches
    .iter()
    .flat_map(|batch| {
      let column = batch.column_by_name("name").unwrap();
      (0..batch.num_rows()).map(move |index| string_value(column.as_ref(), index))
    })
    .collect::<Vec<_>>();
  names.sort();
  assert_eq!(
    names,
    vec!["five".to_string(), "one".to_string(), "three".to_string()]
  );

  let files = parquet_files(&output);
  assert_eq!(files.len(), 2);
  for file in files {
    let schema = raw_parquet_schema(&file);
    assert!(schema.field_with_name("bbox").is_ok());
    let metadata = kv_map(&file);
    let geo: serde_json::Value = serde_json::from_str(metadata.get("geo").unwrap()).unwrap();
    assert_covering_metadata(&geo);
    assert_json_extent(&geo["columns"]["geometry"]["bbox"], [1.0, 1.0, 5.0, 5.0]);

    let reader_metadata = reader_metadata(&file);
    assert!(
      reader_metadata
        .metadata()
        .row_groups()
        .iter()
        .flat_map(|row_group| row_group.columns())
        .all(|column| matches!(column.compression(), Compression::ZSTD(_)))
    );
  }

  for batch in batches {
    let bbox = batch
      .column_by_name("bbox")
      .unwrap()
      .as_any()
      .downcast_ref::<StructArray>()
      .unwrap();
    assert_eq!(bbox.len(), batch.num_rows());
  }
}

#[test]
fn partitioned_output_requires_overwrite_for_existing_destination() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points.parquet");
  let output = temp.path().join("out");
  write_point_input(&input, &[(0.0, 0.0)], &["point"], 4326);
  std::fs::create_dir_all(&output).unwrap();
  let sentinel = output.join("sentinel.txt");
  let stale = output.join("stale.parquet");
  std::fs::write(&sentinel, "preserve-me").unwrap();
  std::fs::write(&stale, "stale-parquet").unwrap();

  let error = runtime()
    .block_on(Pipeline::run(SpatialPipelineOptions {
      input: InputOptions {
        location: input.to_string_lossy().into_owned(),
        ..Default::default()
      },
      output: OutputOptions {
        path: output.clone(),
        mode: OutputMode::Optimized,
        file_count: Some(2),
        ..Default::default()
      },
      ..Default::default()
    }))
    .unwrap_err();

  assert!(error.to_string().contains("output path already exists"));
  assert_eq!(std::fs::read_to_string(&sentinel).unwrap(), "preserve-me");
  assert_eq!(std::fs::read_to_string(&stale).unwrap(), "stale-parquet");
  assert_eq!(std::fs::read_dir(&output).unwrap().count(), 2);
}

#[test]
fn partitioned_output_replaces_existing_destination() {
  let temp = TempDir::new().unwrap();
  let input = temp.path().join("points.parquet");
  let output = temp.path().join("out");
  write_point_input(
    &input,
    &[(0.0, 0.0), (1.0, 1.0), (2.0, 2.0)],
    &["zero", "one", "two"],
    4326,
  );
  std::fs::create_dir_all(output.join("stale-directory")).unwrap();
  std::fs::write(output.join("stale.parquet"), "stale").unwrap();
  std::fs::write(output.join("stale-directory/marker.txt"), "stale").unwrap();

  let result = runtime()
    .block_on(Pipeline::run(SpatialPipelineOptions {
      input: InputOptions {
        location: input.to_string_lossy().into_owned(),
        ..Default::default()
      },
      output: OutputOptions {
        path: output.clone(),
        mode: OutputMode::Optimized,
        file_count: Some(2),
        overwrite: true,
        ..Default::default()
      },
      ..Default::default()
    }))
    .unwrap();

  assert_eq!(result.rows_written(), 3);
  assert!(!output.join("stale.parquet").exists());
  assert!(!output.join("stale-directory").exists());
  assert_eq!(parquet_files(&output).len(), 2);
  assert_eq!(
    std::fs::read_dir(&output)
      .unwrap()
      .filter(|entry| entry.as_ref().unwrap().path().is_dir())
      .count(),
    2
  );
  assert!(!validate(&output).unwrap().has_errors());
}

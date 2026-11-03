// Copyright 2026 Esri
//
// Licensed under the Apache License, Version 2.0 (the "License");
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

mod common;

use std::sync::Arc;

use arrow_array::{BinaryArray, Int32Array, RecordBatch};
use arrow_schema::{DataType, Field, Schema};
use spatial::{
  GeometryDeclaration, GeometryType, InputOptions, OutputMode, OutputOptions, Pipeline,
  PipelineSource, RowRange, SpatialPipelineOptions,
};
use tempfile::TempDir;
use tokio::runtime::Runtime;

use common::assertion::binary_value;
use common::fixture::{wkb_point, wkb_xyzm_point};
use common::parquet::{geoparquet_kv, kv_map, reader_metadata, scan_parquet, write_parquet};

/// Create batches with distinct geometry keys and one null geometry.
fn point_batches() -> Vec<RecordBatch> {
  let schema = Arc::new(Schema::new(vec![
    Field::new("id", DataType::Int32, false),
    Field::new("GEOMETRY", DataType::Binary, true),
  ]));
  let late = wkb_point(8.0, 8.0);
  let early = wkb_point(1.0, 1.0);
  vec![
    RecordBatch::try_new(
      Arc::clone(&schema),
      vec![
        Arc::new(Int32Array::from(vec![0, 1])),
        Arc::new(BinaryArray::from(vec![Some(late.as_slice()), None])),
      ],
    )
    .unwrap(),
    RecordBatch::try_new(
      schema,
      vec![
        Arc::new(Int32Array::from(vec![2])),
        Arc::new(BinaryArray::from(vec![Some(early.as_slice())])),
      ],
    )
    .unwrap(),
  ]
}

/// Compare both source variants through selection, sorting, metadata and real file output.
#[test]
fn arrow_and_dataset_inputs_produce_equivalent_output() {
  let runtime = Runtime::new().unwrap();
  let directory = TempDir::new().unwrap();
  let batches = point_batches();
  let schema = batches[0].schema();
  let input = directory.path().join("input.parquet");
  write_parquet(
    &input,
    &schema,
    &batches,
    parquet::basic::Compression::SNAPPY,
    &[geoparquet_kv("GEOMETRY", &["Point"])],
  );
  for mode in [OutputMode::Plain, OutputMode::Optimized] {
    let mut results = Vec::new();
    for (name, source) in [
      ("dataset", PipelineSource::dataset(input.to_string_lossy())),
      (
        "arrow",
        PipelineSource::Arrow {
          schema: Arc::clone(&schema),
          batches: batches.clone(),
          geometry: GeometryDeclaration::wkb("GEOMETRY", vec![GeometryType::Point], 4326).unwrap(),
        },
      ),
    ] {
      let output = directory.path().join(format!("{name}-{mode:?}.parquet"));
      let result = runtime
        .block_on(Pipeline::run(SpatialPipelineOptions {
          input: InputOptions {
            source,
            row_range: RowRange::new(1, Some(2)),
            ..Default::default()
          },
          output: OutputOptions {
            path: output.clone(),
            mode,
            normalization_extent: Some([0.0, 0.0, 10.0, 10.0]),
            row_group_size: Some(1),
            page_size: Some(32),
            ..Default::default()
          },
          ..Default::default()
        }))
        .unwrap();
      assert_eq!(result.rows_written(), 2);
      assert_eq!(result.rows_expected(), 2);
      let metadata = reader_metadata(&output);
      assert_eq!(metadata.metadata().num_row_groups(), 2);
      let dataframe = runtime
        .block_on(scan_parquet(output.to_str().unwrap()))
        .unwrap();
      let rows = runtime.block_on(dataframe.collect()).unwrap();
      let values: Vec<_> = rows
        .iter()
        .flat_map(|batch| -> Vec<(i32, Option<Vec<u8>>)> {
          let ids = batch
            .column_by_name("id")
            .unwrap()
            .as_any()
            .downcast_ref::<Int32Array>()
            .unwrap();
          let geometry = batch.column_by_name("GEOMETRY").unwrap();
          (0..batch.num_rows())
            .map(|index| -> (i32, Option<Vec<u8>>) {
              (
                ids.value(index),
                (!geometry.is_null(index))
                  .then(|| -> Vec<u8> { binary_value(geometry.as_ref(), index) }),
              )
            })
            .collect()
        })
        .collect();
      let mut entries = kv_map(&output);
      // Dataset scans may use BinaryView while supplied Arrow batches use Binary.
      entries.remove("ARROW:schema");
      results.push((values, entries));
    }
    assert_eq!(results[0], results[1]);
  }
}

/// Keep declared point types while omitting fictitious observed bounds for null or empty input.
#[test]
fn arrow_null_and_empty_geometry_require_a_normalization_domain() {
  let runtime = Runtime::new().unwrap();
  let directory = TempDir::new().unwrap();
  let schema = Arc::new(Schema::new(vec![Field::new(
    "GEOMETRY",
    DataType::Binary,
    true,
  )]));
  for row_count in [0, 3] {
    let batches = if row_count == 0 {
      Vec::new()
    } else {
      vec![
        RecordBatch::try_new(
          Arc::clone(&schema),
          vec![Arc::new(BinaryArray::from(vec![None::<&[u8]>; row_count]))],
        )
        .unwrap(),
      ]
    };
    for extent in [None, Some([-10.0, -20.0, 10.0, 20.0])] {
      let output = directory
        .path()
        .join(format!("null-{row_count}-{}.parquet", extent.is_some()));
      let result = runtime.block_on(Pipeline::run(SpatialPipelineOptions {
        input: InputOptions {
          source: PipelineSource::Arrow {
            schema: Arc::clone(&schema),
            batches: batches.clone(),
            geometry: GeometryDeclaration::wkb("GEOMETRY", vec![GeometryType::Point], 4326)
              .unwrap(),
          },
          ..Default::default()
        },
        output: OutputOptions {
          path: output.clone(),
          normalization_extent: extent,
          ..Default::default()
        },
        ..Default::default()
      }));
      if extent.is_none() {
        assert!(
          result
            .unwrap_err()
            .to_string()
            .contains("normalization_extent")
        );
        assert!(!output.exists());
        continue;
      }
      assert_eq!(
        result.unwrap().rows_written(),
        u64::try_from(row_count).unwrap()
      );
      let entries = kv_map(&output);
      let geo: serde_json::Value = serde_json::from_str(&entries["geo"]).unwrap();
      assert!(geo["columns"]["GEOMETRY"].get("bbox").is_none());
      assert_eq!(
        geo["columns"]["GEOMETRY"]["geometry_types"],
        serde_json::json!(["Point"])
      );
      let display: serde_json::Value = serde_json::from_str(&entries["geodisplay"]).unwrap();
      assert_eq!(
        display["fullExtent"],
        serde_json::json!({
          "xmin": -10.0, "ymin": -20.0, "xmax": 10.0, "ymax": 20.0,
        })
      );
      spatial::validate(&output).unwrap().ensure_valid().unwrap();
    }
  }
}

/// Reject mismatched batch schemas and geometry declarations before creating output.
#[test]
fn arrow_input_rejects_incompatible_values() {
  let runtime = Runtime::new().unwrap();
  let directory = TempDir::new().unwrap();
  let batches = point_batches();
  let wrong_schema = Arc::new(Schema::new(vec![Field::new(
    "GEOMETRY",
    DataType::Binary,
    true,
  )]));
  let dimensional_point = wkb_xyzm_point(1.0, 2.0, Some(3.0), Some(4.0));
  let wrong_dimensions = RecordBatch::try_new(
    Arc::clone(&wrong_schema),
    vec![Arc::new(BinaryArray::from(vec![Some(
      dimensional_point.as_slice(),
    )]))],
  )
  .unwrap();
  for (schema, batches, geometry_type) in [
    (
      Arc::clone(&wrong_schema),
      batches.clone(),
      GeometryType::Point,
    ),
    (batches[0].schema(), batches, GeometryType::Polygon),
    (wrong_schema, vec![wrong_dimensions], GeometryType::Point),
  ] {
    let output = directory.path().join("invalid.parquet");
    let result = runtime.block_on(Pipeline::run(SpatialPipelineOptions {
      input: InputOptions {
        source: PipelineSource::Arrow {
          schema,
          batches,
          geometry: GeometryDeclaration::wkb("GEOMETRY", vec![geometry_type], 4326).unwrap(),
        },
        ..Default::default()
      },
      output: OutputOptions {
        path: output.clone(),
        ..Default::default()
      },
      ..Default::default()
    }));
    assert!(matches!(result, Err(spatial::PipelineError::Input(_))));
    assert!(!output.exists());
  }
}

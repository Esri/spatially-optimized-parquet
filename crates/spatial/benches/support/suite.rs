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

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use criterion::{BenchmarkGroup, BenchmarkId, Criterion, SamplingMode, Throughput};
use spatial::{InputOptions, OutputMode, OutputOptions, Pipeline, SpatialPipelineOptions};
use tokio::runtime::Runtime;

use super::{BenchmarkFixture, BenchmarkFixtureSet, remove_output};

#[derive(Clone, Copy)]
struct BenchmarkCase {
  name: &'static str,
  mode: OutputMode,
  output_files: Option<usize>,
  covering: bool,
}

const OPTIMIZED_SINGLE: BenchmarkCase = BenchmarkCase {
  name: "optimized_single",
  mode: OutputMode::Optimized,
  output_files: None,
  covering: true,
};
const OPTIMIZED_PARTITIONED: BenchmarkCase = BenchmarkCase {
  name: "optimized_partitioned_8",
  mode: OutputMode::Optimized,
  output_files: Some(8),
  covering: true,
};

/// Benchmark standard and wide fixtures with their selected single-file and partitioned cases.
pub fn benchmark_writer(criterion: &mut Criterion, target_mib: u64, suite_name: &str) {
  let fixtures = BenchmarkFixtureSet::build(target_mib);
  let runtime = Runtime::new().expect("create benchmark runtime");
  benchmark_geometry(
    criterion,
    &runtime,
    &fixtures.point,
    &fixtures.output_root,
    suite_name,
    "point",
    &[OPTIMIZED_SINGLE, OPTIMIZED_PARTITIONED],
  );
  benchmark_geometry(
    criterion,
    &runtime,
    &fixtures.polygon,
    &fixtures.output_root,
    suite_name,
    "polygon",
    &[OPTIMIZED_SINGLE, OPTIMIZED_PARTITIONED],
  );
  benchmark_geometry(
    criterion,
    &runtime,
    &fixtures.wide_polygon,
    &fixtures.output_root,
    suite_name,
    "polygon_wide_4x",
    &[OPTIMIZED_SINGLE],
  );
}

/// Configure ten samples with bounded warmup and measurement durations.
pub fn criterion_config() -> Criterion {
  Criterion::default()
    .sample_size(10)
    .warm_up_time(Duration::from_secs(2))
    .measurement_time(Duration::from_secs(20))
}

/// Register the selected writer cases with byte throughput for one geometry fixture.
fn benchmark_geometry(
  criterion: &mut Criterion,
  runtime: &Runtime,
  fixture: &BenchmarkFixture,
  output_root: &Path,
  suite_name: &str,
  geometry_name: &str,
  cases: &[BenchmarkCase],
) {
  let mut group = criterion.benchmark_group(format!("writer/{suite_name}/{geometry_name}"));
  group.sampling_mode(SamplingMode::Flat);
  group.throughput(Throughput::Bytes(fixture.file_bytes));
  for case in cases {
    benchmark_case(&mut group, runtime, fixture, output_root, *case);
  }
  group.finish();
}

/// Measure pipeline execution while excluding option construction and output cleanup.
fn benchmark_case(
  group: &mut BenchmarkGroup<'_, criterion::measurement::WallTime>,
  runtime: &Runtime,
  fixture: &BenchmarkFixture,
  output_root: &Path,
  case: BenchmarkCase,
) {
  group.bench_with_input(
    BenchmarkId::new(case.name, fixture.row_count),
    &case,
    |bencher, case| {
      let mut output_index = 0_u64;
      bencher.iter_custom(|iterations| {
        let mut measured = Duration::ZERO;
        for _ in 0..iterations {
          let output = output_path(output_root, case, output_index);
          output_index += 1;
          let options = SpatialPipelineOptions {
            input: InputOptions {
              location: fixture.path.to_string_lossy().into_owned(),
              ..Default::default()
            },
            output: OutputOptions {
              path: output.clone(),
              mode: case.mode,
              file_count: case.output_files,
              compression: Some("snappy".to_string()),
              covering: case.covering,
              overwrite: true,
              ..Default::default()
            },
            ..Default::default()
          };
          let started = Instant::now();
          runtime
            .block_on(Pipeline::run(options))
            .expect("execute writer benchmark");
          measured += started.elapsed();
          remove_output(&output);
        }
        measured
      });
    },
  );
}

/// Choose a unique directory or Parquet file path for one measured iteration.
fn output_path(root: &Path, case: &BenchmarkCase, index: u64) -> PathBuf {
  match case.output_files {
    Some(_) => root.join(format!("{}-{index}", case.name)),
    None => root.join(format!("{}-{index}.parquet", case.name)),
  }
}

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use criterion::{BenchmarkGroup, BenchmarkId, Criterion, SamplingMode, Throughput};
use spatial::{InputOptions, OutputMode, OutputOptions, RowRange, SpatialPipelineOptions, run};
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
  mode: OutputMode::OptimizedGeoParquet,
  output_files: None,
  covering: true,
};
const OPTIMIZED_PARTITIONED: BenchmarkCase = BenchmarkCase {
  name: "optimized_partitioned_8",
  mode: OutputMode::OptimizedGeoParquet,
  output_files: Some(8),
  covering: true,
};

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
  );
  benchmark_geometry(
    criterion,
    &runtime,
    &fixtures.polygon,
    &fixtures.output_root,
    suite_name,
    "polygon",
  );
  benchmark_single_file(
    criterion,
    &runtime,
    &fixtures.wide_polygon,
    &fixtures.output_root,
    suite_name,
    "polygon_wide_4x",
  );
}

fn benchmark_geometry(
  criterion: &mut Criterion,
  runtime: &Runtime,
  fixture: &BenchmarkFixture,
  output_root: &Path,
  suite_name: &str,
  geometry_name: &str,
) {
  let mut group = criterion.benchmark_group(format!("writer/{suite_name}/{geometry_name}"));
  group.sampling_mode(SamplingMode::Flat);
  group.throughput(Throughput::Bytes(fixture.file_bytes));
  for case in [OPTIMIZED_SINGLE, OPTIMIZED_PARTITIONED] {
    benchmark_case(&mut group, runtime, fixture, output_root, case);
  }
  group.finish();
}

fn benchmark_single_file(
  criterion: &mut Criterion,
  runtime: &Runtime,
  fixture: &BenchmarkFixture,
  output_root: &Path,
  suite_name: &str,
  geometry_name: &str,
) {
  let mut group = criterion.benchmark_group(format!("writer/{suite_name}/{geometry_name}"));
  group.sampling_mode(SamplingMode::Flat);
  group.throughput(Throughput::Bytes(fixture.file_bytes));
  benchmark_case(&mut group, runtime, fixture, output_root, OPTIMIZED_SINGLE);
  group.finish();
}

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
          let options = SpatialPipelineOptions::new(
            InputOptions::new(
              fixture.path.to_string_lossy().into_owned(),
              None,
              RowRange::default(),
              None,
              None,
              None,
            ),
            OutputOptions::new(
              &output,
              case.mode,
              case.output_files,
              Some("snappy".to_string()),
              4326,
              case.covering,
              true,
            ),
          );
          let started = Instant::now();
          runtime
            .block_on(run(options))
            .expect("execute writer benchmark");
          measured += started.elapsed();
          remove_output(&output);
        }
        measured
      });
    },
  );
}

fn output_path(root: &Path, case: &BenchmarkCase, index: u64) -> PathBuf {
  match case.output_files {
    Some(_) => root.join(format!("{}-{index}", case.name)),
    None => root.join(format!("{}-{index}.parquet", case.name)),
  }
}

pub fn criterion_config() -> Criterion {
  Criterion::default()
    .sample_size(10)
    .warm_up_time(Duration::from_secs(2))
    .measurement_time(Duration::from_secs(20))
}

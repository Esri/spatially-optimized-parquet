//! Defines the `sop` write and validation process boundary.
//!
//! This module deliberately contains no storage-format or geometry logic. Clap validates
//! argument shape, the local parsers enforce row-range constraints, and [`run`] maps the
//! resulting values into domain options. The `spatial` crate then owns input detection,
//! metadata discovery, DataFusion planning, geometry transformation, and durable output.
//!
//! Keeping this layer thin prevents CLI concerns from leaking into reusable pipeline code. A
//! failure returned by the pipeline propagates through `main`, producing a non-zero process exit.

mod write_progress;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Parser, Subcommand, ValueEnum};
use spatial::{
  DEFAULT_OUTPUT_WKID, InputOptions, MultiscaleEncoding, OutputMode, OutputOptions, RowRange,
  SourceFormat, SpatialPipelineOptions, ValidationReport,
};

use crate::write_progress::StdoutWriteReporter;

#[derive(Parser, Debug)]
#[command(
  author,
  version,
  about = "Write and validate spatially optimized GeoParquet output"
)]
struct Cli {
  #[command(subcommand)]
  command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
  /// Generate spatially optimized GeoParquet output.
  Write(WriteCommand),
  /// Validate one optimized Parquet file or recursive dataset directory.
  Validate(ValidateCommand),
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum MultiscaleEncodingValue {
  #[default]
  Pbf,
  QuantizedNative,
}

impl From<MultiscaleEncodingValue> for MultiscaleEncoding {
  fn from(value: MultiscaleEncodingValue) -> Self {
    match value {
      MultiscaleEncodingValue::Pbf => Self::Pbf,
      MultiscaleEncodingValue::QuantizedNative => Self::QuantizedNative,
    }
  }
}

#[derive(Args, Debug)]
struct WriteCommand {
  #[arg(long, value_name = "PATH")]
  input: String,
  #[arg(long, value_name = "FORMAT")]
  input_format: Option<SourceFormat>,
  #[arg(long, value_name = "PATH")]
  output: PathBuf,
  #[arg(long, value_name = "N")]
  output_files: Option<usize>,
  #[arg(
    long = "memory",
    value_name = "GB",
    value_parser = parse_memory_gb,
    help = "Set the DataFusion memory pool in whole GiB; defaults to half of total physical memory"
  )]
  memory_limit_bytes: Option<usize>,
  #[arg(
    long = "sort-concurrency",
    value_name = "N",
    value_parser = parse_sort_concurrency,
    help = "Set DataFusion sort concurrency; defaults to the available CPU core count"
  )]
  sort_concurrency: Option<usize>,
  #[arg(
    long,
    value_name = "N",
    value_parser = parse_cores,
    help = "Set Tokio worker threads; defaults to the available CPU core count"
  )]
  cores: Option<usize>,
  #[arg(long, value_name = "STRING")]
  compression: Option<String>,
  #[arg(long, value_name = "N", value_parser = parse_start, help = "Skip the first N input rows before processing")]
  start: Option<usize>,
  #[arg(long, value_name = "N", value_parser = parse_num, help = "Process at most N input rows")]
  num: Option<usize>,
  #[arg(
    long,
    value_name = "NAME",
    help = "Select a layer from a multi-layer input such as a GeoPackage"
  )]
  layer: Option<String>,
  #[arg(long, value_name = "NAME")]
  geometry_column: Option<String>,
  #[arg(
    long,
    value_name = "LATEST_WKID",
    help = "Set the input CRS when source geometry metadata does not declare one"
  )]
  in_sr: Option<u32>,
  #[arg(
    long,
    value_name = "LATEST_WKID",
    default_value_t = DEFAULT_OUTPUT_WKID,
    help = "Set the output CRS"
  )]
  out_sr: u32,
  #[arg(
    long,
    help = "Write a root bbox struct column and GeoParquet 1.1 covering metadata"
  )]
  covering: bool,
  #[arg(long, help = "Remove Z values from output geometry and metadata")]
  strip_z: bool,
  #[arg(long, help = "Remove M values from output geometry and metadata")]
  strip_m: bool,
  #[arg(
    long,
    help = "Overwrite the output file or replace the output directory if it exists"
  )]
  overwrite: bool,
  #[arg(
    long,
    help = "Suppress live write updates while retaining the final written feature count"
  )]
  no_progress: bool,
  #[arg(
    long = "no-optimization",
    help = "Pass through the selected input rows without sorting, display optimization, or geodisplay metadata changes"
  )]
  no_optimization: bool,
  #[arg(
    long,
    value_enum,
    default_value_t,
    help = "EXPERIMENTAL: Select the optimized multiscale geometry encoding"
  )]
  multiscale_encoding: MultiscaleEncodingValue,
}

#[derive(Args, Debug)]
struct ValidateCommand {
  #[arg(value_name = "FILE_OR_DIRECTORY")]
  path: PathBuf,
}

fn main() -> Result<()> {
  let cli = Cli::parse();
  let worker_threads = match &cli.command {
    Command::Write(args) => args.cores,
    Command::Validate(_) => None,
  };
  let mut runtime = tokio::runtime::Builder::new_multi_thread();
  runtime.enable_all();
  if let Some(worker_threads) = worker_threads {
    runtime.worker_threads(worker_threads);
  }
  runtime.build()?.block_on(run(cli))
}

async fn run(cli: Cli) -> Result<()> {
  match cli.command {
    Command::Write(args) => {
      let reporter = StdoutWriteReporter::new(!args.no_progress);
      let options = SpatialPipelineOptions::from(args).with_write_reporter(reporter.clone());
      let result = spatial::run(options).await?;
      reporter.finish(result.rows_written(), result.rows_expected());
      for warning in result.warnings() {
        eprintln!("{warning}");
      }
      Ok(())
    }
    Command::Validate(args) => match spatial::validate(&args.path)?.ensure_valid() {
      Ok(report) => {
        render_validation_report(&report);
        Ok(())
      }
      Err(failure) => Err(failure.into()),
    },
  }
}

impl From<WriteCommand> for SpatialPipelineOptions {
  fn from(args: WriteCommand) -> Self {
    let memory_limit_bytes = args.memory_limit_bytes;
    let sort_concurrency = args.sort_concurrency;
    let output_mode = if args.no_optimization {
      OutputMode::Plain
    } else {
      OutputMode::Optimized
    };
    let mut options = Self::new(
      InputOptions::new(
        args.input,
        args.input_format,
        RowRange::new(args.start.unwrap_or(0), args.num),
        args.layer,
        args.geometry_column,
        args.in_sr,
      ),
      OutputOptions::new(
        args.output,
        output_mode,
        args.output_files,
        args.compression,
        args.out_sr,
        args.covering,
        args.overwrite,
      )
      .with_stripped_dimensions(args.strip_z, args.strip_m)
      .with_multiscale_encoding(args.multiscale_encoding.into()),
    );
    if let Some(memory_limit_bytes) = memory_limit_bytes {
      options = options.with_memory_limit_bytes(memory_limit_bytes);
    }
    if let Some(sort_concurrency) = sort_concurrency {
      options = options.with_target_partitions(sort_concurrency);
    }
    options
  }
}

fn render_validation_report(report: &ValidationReport) {
  print!("{report}");
}

/// Parse a zero-based input row offset.
fn parse_start(value: &str) -> Result<usize, String> {
  value
    .parse::<usize>()
    .map_err(|_| format!("invalid value for --start: {value}"))
}

/// Parse a non-zero maximum row count.
fn parse_num(value: &str) -> Result<usize, String> {
  let num = value
    .parse::<usize>()
    .map_err(|_| format!("invalid value for --num: {value}"))?;
  if num == 0 {
    return Err("--num must be >= 1".to_string());
  }
  Ok(num)
}

/// Parse a non-zero whole-GiB DataFusion memory limit.
fn parse_memory_gb(value: &str) -> Result<usize, String> {
  let memory_gb = parse_positive_usize(value, "--memory")?;
  memory_gb
    .checked_mul(1024 * 1024 * 1024)
    .ok_or_else(|| format!("value for --memory is too large: {value}"))
}

/// Parse a non-zero DataFusion sort concurrency.
fn parse_sort_concurrency(value: &str) -> Result<usize, String> {
  parse_positive_usize(value, "--sort-concurrency")
}

/// Parse a non-zero Tokio worker-thread count.
fn parse_cores(value: &str) -> Result<usize, String> {
  parse_positive_usize(value, "--cores")
}

fn parse_positive_usize(value: &str, option: &str) -> Result<usize, String> {
  let parsed = value
    .parse::<usize>()
    .map_err(|_| format!("invalid value for {option}: {value}"))?;
  if parsed == 0 {
    return Err(format!("{option} must be >= 1"));
  }
  Ok(parsed)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn write_subcommand_accepts_dimension_stripping_flags() {
    let cli = Cli::try_parse_from([
      "sop",
      "write",
      "--input",
      "input.parquet",
      "--output",
      "output.parquet",
      "--strip-z",
      "--strip-m",
    ])
    .unwrap();

    let Command::Write(args) = cli.command else {
      panic!("expected write command");
    };
    assert!(args.strip_z);
    assert!(args.strip_m);
  }

  #[test]
  fn write_subcommand_rejects_zero_resource_limits() {
    for option in ["--memory", "--sort-concurrency", "--cores"] {
      let error = Cli::try_parse_from([
        "sop",
        "write",
        "--input",
        "input.parquet",
        "--output",
        "output.parquet",
        option,
        "0",
      ])
      .unwrap_err();

      assert!(error.to_string().contains("must be >= 1"));
    }
  }
}

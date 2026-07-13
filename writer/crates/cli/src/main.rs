//! Defines the `parquet-opt` process boundary and translates command-line arguments into
//! one [`OptimizeJobOptions`] request.
//!
//! This module deliberately contains no storage-format or geometry logic. Clap validates
//! argument shape, the local parsers enforce row-range constraints, and [`run`] maps the
//! resulting values into domain options. The `spatial` crate then owns input detection,
//! metadata discovery, DataFusion planning, geometry transformation, progress reporting,
//! and durable output.
//!
//! Keeping this layer thin prevents CLI concerns from leaking into reusable job code. A
//! failure returned by the job propagates through `main`, producing a non-zero process exit.

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use spatial::input::{RowRange, SourceFormat};
use spatial::job::{OptimizeJobOptions, run_optimize_job};
use spatial::output::GeoParquetOutputMode;

#[derive(Parser, Debug)]
#[command(
  author,
  version,
  about = "Generate spatially optimized GeoParquet output"
)]
struct Cli {
  #[arg(long, value_name = "PATH")]
  input: String,
  #[arg(long, value_name = "FORMAT")]
  input_format: Option<SourceFormat>,
  #[arg(long, value_name = "PATH")]
  output: PathBuf,
  #[arg(long, value_name = "N")]
  output_files: Option<usize>,
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
    help = "Write a root bbox struct column and GeoParquet 1.1 covering metadata"
  )]
  covering: bool,
  #[arg(
    long,
    help = "Overwrite the output file or replace the output directory if it exists"
  )]
  overwrite: bool,
  #[arg(
    long,
    help = "Print detailed plan and performance diagnostics to stderr, including DataFusion EXPLAIN VERBOSE / EXPLAIN ANALYZE VERBOSE-style output and per-operator metrics; disables progress bars and can be substantially slower"
  )]
  explain: bool,
  #[arg(
    long = "no-optimiztaion",
    alias = "no-optimization",
    help = "Pass through the selected input rows without sorting, display optimization, or geodisplay metadata changes"
  )]
  no_optimization: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
  let cli = Cli::parse();
  run(cli).await
}

async fn run(cli: Cli) -> Result<()> {
  let options = OptimizeJobOptions {
    input: cli.input,
    input_format: cli.input_format,
    output: cli.output,
    output_files: cli.output_files,
    compression: cli.compression,
    row_range: RowRange {
      start: cli.start.unwrap_or(0),
      num: cli.num,
    },
    layer: cli.layer,
    geometry_column: cli.geometry_column,
    input_wkid: cli.in_sr,
    covering: cli.covering,
    overwrite: cli.overwrite,
    progress: !cli.explain,
    explain: cli.explain,
    output_mode: if cli.no_optimization {
      GeoParquetOutputMode::Plain
    } else {
      GeoParquetOutputMode::Optimized
    },
  };
  run_optimize_job(options).await
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

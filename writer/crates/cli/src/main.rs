use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use spatial::input::RowRange;
use spatial::job::{OptimizeJobOptions, run_optimize_job};

#[derive(Parser, Debug)]
#[command(
  author,
  version,
  about = "Generate spatially optimized GeoParquet output"
)]
struct Cli {
  #[arg(long, value_name = "PATH")]
  input: String,
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
    output: cli.output,
    output_files: cli.output_files,
    compression: cli.compression,
    row_range: RowRange {
      start: cli.start.unwrap_or(0),
      num: cli.num,
    },
    layer: cli.layer,
    geometry_column: cli.geometry_column,
    covering: cli.covering,
    overwrite: cli.overwrite,
    progress: !cli.explain,
    explain: cli.explain,
    no_optimization: cli.no_optimization,
  };
  run_optimize_job(options).await
}

fn parse_start(value: &str) -> Result<usize, String> {
  value
    .parse::<usize>()
    .map_err(|_| format!("invalid value for --start: {value}"))
}

fn parse_num(value: &str) -> Result<usize, String> {
  let num = value
    .parse::<usize>()
    .map_err(|_| format!("invalid value for --num: {value}"))?;
  if num == 0 {
    return Err("--num must be >= 1".to_string());
  }
  Ok(num)
}

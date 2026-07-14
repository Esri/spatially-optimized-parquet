//! Defines the `parquet-opt` write and validation process boundary.
//!
//! This module deliberately contains no storage-format or geometry logic. Clap validates
//! argument shape, the local parsers enforce row-range constraints, and [`run`] maps the
//! resulting values into domain options. The `spatial` crate then owns input detection,
//! metadata discovery, DataFusion planning, geometry transformation, progress reporting,
//! and durable output.
//!
//! Keeping this layer thin prevents CLI concerns from leaking into reusable pipeline code. A
//! failure returned by the pipeline propagates through `main`, producing a non-zero process exit.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use spatial::{
  DEFAULT_OUTPUT_WKID, ExecutionOptions, InputOptions, OutputMode, OutputOptions, RowRange,
  SourceFormat, SpatialPipelineOptions, ValidationReport,
};

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
    long = "no-optimization",
    alias = "no-optimiztaion",
    help = "Pass through the selected input rows without sorting, display optimization, or geodisplay metadata changes"
  )]
  no_optimization: bool,
}

#[derive(Args, Debug)]
struct ValidateCommand {
  #[arg(value_name = "FILE_OR_DIRECTORY")]
  path: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
  let cli = Cli::parse();
  run(cli).await
}

async fn run(cli: Cli) -> Result<()> {
  match cli.command {
    Command::Write(args) => {
      let result = spatial::run(args.into()).await?;
      println!("wrote {} rows", result.rows_written());
      if let Some(report) = result.validation_report() {
        render_validation_report(report);
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
    let output_mode = if args.no_optimization {
      OutputMode::Plain
    } else {
      OutputMode::Optimized
    };
    Self::new(
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
      ),
      ExecutionOptions::new(!args.explain, args.explain),
    )
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn write_subcommand_preserves_existing_arguments() {
    let cli = Cli::try_parse_from([
      "parquet-opt",
      "write",
      "--input",
      "input.parquet",
      "--output",
      "output.parquet",
      "--no-optimization",
    ])
    .unwrap();

    let Command::Write(args) = cli.command else {
      panic!("expected write command");
    };
    assert_eq!(args.input, "input.parquet");
    assert_eq!(args.output, PathBuf::from("output.parquet"));
    assert!(args.no_optimization);
  }

  #[test]
  fn validate_subcommand_accepts_one_path() {
    let cli =
      Cli::try_parse_from(["parquet-opt", "validate", "output"]).expect("validate arguments");

    let Command::Validate(args) = cli.command else {
      panic!("expected validate command");
    };
    assert_eq!(args.path, PathBuf::from("output"));
  }
}

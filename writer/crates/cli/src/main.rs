//! Defines the `parquet-opt` write and validation process boundary.
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
use clap::{Args, Parser, Subcommand};
use spatial::{
  DEFAULT_OUTPUT_WKID, InputOptions, OutputMode, OutputOptions, RowRange, SourceFormat,
  SpatialPipelineOptions, ValidationReport,
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
    help = "Suppress live write updates while retaining the final written feature count"
  )]
  no_progress: bool,
  #[arg(
    long = "no-optimization",
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
      let reporter = StdoutWriteReporter::new(!args.no_progress);
      let options = SpatialPipelineOptions::from(args).with_write_reporter(reporter.clone());
      let result = spatial::run(options).await?;
      reporter.finish(result.rows_written(), result.rows_expected());
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
  fn write_subcommand_accepts_no_progress() {
    let cli = Cli::try_parse_from([
      "parquet-opt",
      "write",
      "--input",
      "input.parquet",
      "--output",
      "output.parquet",
      "--no-progress",
    ])
    .unwrap();

    let Command::Write(args) = cli.command else {
      panic!("expected write command");
    };
    assert!(args.no_progress);
  }

  #[test]
  fn write_subcommand_rejects_removed_explain() {
    let error = Cli::try_parse_from([
      "parquet-opt",
      "write",
      "--input",
      "input.parquet",
      "--output",
      "output.parquet",
      "--explain",
    ])
    .unwrap_err();

    assert!(
      error
        .to_string()
        .contains("unexpected argument '--explain'")
    );
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

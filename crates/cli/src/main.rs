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
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Args, Parser, Subcommand, ValueEnum};
use spatial::{
  DEFAULT_OUTPUT_WKID, InputOptions, MultiscaleEncoding, OutputMode, OutputOptions, PipelineError,
  RowRange, SourceFormat, SpatialPipelineOptions, ValidationError, ValidationFailure,
  ValidationReport,
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
  Wkb,
  Native,
}

impl From<MultiscaleEncodingValue> for MultiscaleEncoding {
  fn from(value: MultiscaleEncodingValue) -> Self {
    match value {
      MultiscaleEncodingValue::Pbf => Self::Pbf,
      MultiscaleEncodingValue::Wkb => Self::Wkb,
      MultiscaleEncodingValue::Native => Self::Native,
    }
  }
}

#[derive(Args, Debug)]
struct WriteCommand {
  #[arg(value_name = "INPUT")]
  input: String,
  #[arg(long, value_name = "FORMAT")]
  input_format: Option<SourceFormat>,
  #[arg(short = 'o', long, value_name = "OUTPUT")]
  output: PathBuf,
  #[arg(short = 'p', long, value_name = "N")]
  partitions: Option<usize>,
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
    short = 'l',
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
    value_names = ["XMIN", "YMIN", "XMAX", "YMAX"],
    num_args = 4,
    allow_hyphen_values = true,
    help = "Set the extent used to normalize Z or XZ cluster keys"
  )]
  normalization_extent: Option<Vec<f64>>,
  #[arg(
    long,
    default_value_t = 20,
    value_parser = parse_cluster_depth,
    help = "Set the point Z bit width or non-point XZ maximum level"
  )]
  cluster_depth: u32,
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
    short = 'w',
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
  #[arg(long, help = "Omit SOP geodisplay metadata from optimized output")]
  no_write_sop: bool,
  #[arg(
    long,
    help = "EXPERIMENTAL: Add draft GeoParquet ordering and level-of-detail metadata"
  )]
  write_extensions: bool,
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
  #[arg(value_name = "INPUT")]
  input: PathBuf,
}

#[derive(Debug, thiserror::Error)]
enum CliError {
  #[error(transparent)]
  Runtime(#[from] std::io::Error),
  #[error(transparent)]
  Pipeline(#[from] PipelineError),
  #[error(transparent)]
  Validation(#[from] ValidationError),
  #[error(transparent)]
  InvalidDataset(#[from] ValidationFailure),
}

type CliResult<T> = std::result::Result<T, CliError>;

fn main() -> ExitCode {
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
  let result = runtime
    .build()
    .map_err(CliError::from)
    .and_then(|runtime| runtime.block_on(run(cli)));
  match result {
    Ok(()) => ExitCode::SUCCESS,
    Err(error) => {
      eprintln!("Error: {error}");
      ExitCode::FAILURE
    }
  }
}

async fn run(cli: Cli) -> CliResult<()> {
  match cli.command {
    Command::Write(args) => {
      let reporter = StdoutWriteReporter::new(!args.no_progress);
      let mut options = SpatialPipelineOptions::from(args);
      options.write_reporter = Some(Arc::new(reporter.clone()));
      let result = spatial::Pipeline::run(options).await?;
      reporter.finish(result.rows_written(), result.rows_expected());
      for warning in result.warnings() {
        eprintln!("{warning}");
      }
      Ok(())
    }
    Command::Validate(args) => match spatial::validate(&args.input)?.ensure_valid() {
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
    Self {
      input: InputOptions {
        location: args.input,
        format: args.input_format,
        row_range: RowRange::new(args.start.unwrap_or(0), args.num),
        layer: args.layer,
        geometry_column: args.geometry_column,
        input_wkid: args.in_sr,
      },
      output: OutputOptions {
        path: args.output,
        mode: output_mode,
        file_count: args.partitions,
        compression: args.compression,
        output_wkid: args.out_sr,
        normalization_extent: args.normalization_extent.map(|extent| {
          extent
            .try_into()
            .expect("clap enforces four normalization extent values")
        }),
        cluster_depth: args.cluster_depth,
        covering: args.covering,
        overwrite: args.overwrite,
        strip_z: args.strip_z,
        strip_m: args.strip_m,
        multiscale_encoding: args.multiscale_encoding.into(),
        write_sop: !args.no_write_sop,
        write_extensions: args.write_extensions,
      },
      memory_limit_bytes: args.memory_limit_bytes,
      target_partitions: args.sort_concurrency,
      write_reporter: None,
    }
  }
}

fn render_validation_report(report: &ValidationReport) {
  print!("{report}");
}

/// Parse a zero-based input row offset.
fn parse_start(value: &str) -> std::result::Result<usize, String> {
  value
    .parse::<usize>()
    .map_err(|_| format!("invalid value for --start: {value}"))
}

/// Parse a non-zero maximum row count.
fn parse_num(value: &str) -> std::result::Result<usize, String> {
  let num = value
    .parse::<usize>()
    .map_err(|_| format!("invalid value for --num: {value}"))?;
  if num == 0 {
    return Err("--num must be >= 1".to_string());
  }
  Ok(num)
}

/// Parse a non-zero whole-GiB DataFusion memory limit.
fn parse_memory_gb(value: &str) -> std::result::Result<usize, String> {
  let memory_gb = parse_positive_usize(value, "--memory")?;
  memory_gb
    .checked_mul(1024 * 1024 * 1024)
    .ok_or_else(|| format!("value for --memory is too large: {value}"))
}

/// Parse a non-zero DataFusion sort concurrency.
fn parse_sort_concurrency(value: &str) -> std::result::Result<usize, String> {
  parse_positive_usize(value, "--sort-concurrency")
}

/// Parse a non-zero Tokio worker-thread count.
fn parse_cores(value: &str) -> std::result::Result<usize, String> {
  parse_positive_usize(value, "--cores")
}

fn parse_positive_usize(value: &str, option: &str) -> std::result::Result<usize, String> {
  let parsed = value
    .parse::<usize>()
    .map_err(|_| format!("invalid value for {option}: {value}"))?;
  if parsed == 0 {
    return Err(format!("{option} must be >= 1"));
  }
  Ok(parsed)
}

fn parse_cluster_depth(value: &str) -> std::result::Result<u32, String> {
  let depth = value
    .parse::<u32>()
    .map_err(|_| format!("invalid value for --cluster-depth: {value}"))?;
  if !(1..=32).contains(&depth) {
    return Err("--cluster-depth must be between 1 and 32".to_string());
  }
  Ok(depth)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn write_subcommand_accepts_dimension_stripping_flags() {
    let cli = Cli::try_parse_from([
      "sop",
      "write",
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
  fn write_subcommand_accepts_supported_multiscale_encodings() {
    for encoding in ["pbf", "wkb", "native"] {
      let cli = Cli::try_parse_from([
        "sop",
        "write",
        "input.parquet",
        "--output",
        "output.parquet",
        "--multiscale-encoding",
        encoding,
      ])
      .unwrap();
      let Command::Write(args) = cli.command else {
        panic!("expected write command");
      };
      let _: MultiscaleEncoding = args.multiscale_encoding.into();
    }
  }

  #[test]
  fn write_subcommand_rejects_removed_multiscale_encodings() {
    for encoding in [
      "wkb-quantized",
      "native-quantized",
      "native-quantized-float",
    ] {
      assert!(
        Cli::try_parse_from([
          "sop",
          "write",
          "input.parquet",
          "--output",
          "output.parquet",
          "--multiscale-encoding",
          encoding,
        ])
        .is_err()
      );
    }
  }

  #[test]
  fn write_subcommand_accepts_normalization_extent_and_cluster_depth() {
    let cli = Cli::try_parse_from([
      "sop",
      "write",
      "input.parquet",
      "--output",
      "output.parquet",
      "--normalization-extent",
      "-180",
      "-90",
      "180",
      "90",
      "--cluster-depth",
      "32",
    ])
    .unwrap();

    let Command::Write(args) = cli.command else {
      panic!("expected write command");
    };
    let options = SpatialPipelineOptions::from(args);
    assert_eq!(
      options.output.normalization_extent,
      Some([-180.0, -90.0, 180.0, 90.0])
    );
    assert_eq!(options.output.cluster_depth, 32);
  }

  #[test]
  fn write_subcommand_rejects_cluster_depth_outside_supported_range() {
    for depth in ["0", "33"] {
      let error = Cli::try_parse_from([
        "sop",
        "write",
        "input.parquet",
        "--output",
        "output.parquet",
        "--cluster-depth",
        depth,
      ])
      .unwrap_err();

      assert!(error.to_string().contains("between 1 and 32"));
    }
  }

  #[test]
  fn write_subcommand_writes_sop_by_default_and_accepts_extension_only_metadata() {
    let default_cli = Cli::try_parse_from([
      "sop",
      "write",
      "input.parquet",
      "--output",
      "output.parquet",
    ])
    .unwrap();
    let Command::Write(default_args) = default_cli.command else {
      panic!("expected write command");
    };
    let default_options = SpatialPipelineOptions::from(default_args);
    assert!(default_options.output.write_sop);
    assert!(!default_options.output.write_extensions);

    let extension_cli = Cli::try_parse_from([
      "sop",
      "write",
      "input.parquet",
      "--output",
      "output.parquet",
      "--write-extensions",
      "--no-write-sop",
    ])
    .unwrap();
    let Command::Write(extension_args) = extension_cli.command else {
      panic!("expected write command");
    };
    let extension_options = SpatialPipelineOptions::from(extension_args);
    assert!(!extension_options.output.write_sop);
    assert!(extension_options.output.write_extensions);
  }

  #[test]
  fn write_subcommand_rejects_zero_resource_limits() {
    for option in ["--memory", "--sort-concurrency", "--cores"] {
      let error = Cli::try_parse_from([
        "sop",
        "write",
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

  #[test]
  fn write_subcommand_uses_positional_input_and_short_option_aliases() {
    let cli = Cli::try_parse_from([
      "sop",
      "write",
      "input.parquet",
      "-l",
      "buildings",
      "-o",
      "output",
      "-p",
      "2",
      "-w",
    ])
    .unwrap();

    let Command::Write(args) = cli.command else {
      panic!("expected write command");
    };
    assert_eq!(args.input, "input.parquet");
    assert_eq!(args.layer.as_deref(), Some("buildings"));
    assert_eq!(args.output, PathBuf::from("output"));
    assert_eq!(args.partitions, Some(2));
    assert!(args.overwrite);
  }

  #[test]
  fn write_subcommand_rejects_removed_input_and_output_files_options() {
    for option in ["--input", "--output-files"] {
      let error = Cli::try_parse_from([
        "sop",
        "write",
        "input.parquet",
        "--output",
        "output.parquet",
        option,
        "value",
      ])
      .unwrap_err();

      assert!(error.to_string().contains("unexpected argument"));
    }
  }

  #[test]
  fn validate_subcommand_uses_positional_input() {
    let cli = Cli::try_parse_from(["sop", "validate", "output.parquet"]).unwrap();

    let Command::Validate(args) = cli.command else {
      panic!("expected validate command");
    };
    assert_eq!(args.input, PathBuf::from("output.parquet"));
  }
}

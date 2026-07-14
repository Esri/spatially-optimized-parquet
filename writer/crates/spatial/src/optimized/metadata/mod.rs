//! Exposes optimized geodisplay metadata contracts and writing.

mod writer;

use anyhow::Result;
use parquet::file::metadata::KeyValue;

use super::ResolvedOptimization;

pub(super) fn parquet_metadata(
  optimization: &ResolvedOptimization,
  covering: bool,
) -> Result<Vec<KeyValue>> {
  optimization.parquet_metadata(covering)
}

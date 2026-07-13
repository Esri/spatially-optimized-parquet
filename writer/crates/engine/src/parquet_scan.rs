//! Constructs local Parquet scans through DataFusion.
//!
//! [`scan_parquet`] creates a minimal DataFusion context for local Parquet input without eagerly
//! collecting rows.
//!
//! The main spatial job usually builds its own configured session through `engine::session`.
//! These helpers primarily support format adapters that need a compact read-and-stream boundary.

use anyhow::{Context, Result};
use datafusion::dataframe::DataFrame;
use datafusion::datasource::file_format::options::ParquetReadOptions;
use datafusion::execution::context::SessionContext;
use datafusion_execution::config::SessionConfig;

/// Create a DataFrame over a local Parquet file or directory.
pub async fn scan_parquet(input_path: &str) -> Result<DataFrame> {
  let session_config = SessionConfig::new().with_collect_statistics(false);
  let ctx = SessionContext::new_with_config(session_config);
  ctx
    .read_parquet(input_path, ParquetReadOptions::default())
    .await
    .context("read parquet")
}

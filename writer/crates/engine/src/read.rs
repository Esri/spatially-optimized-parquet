//! Adapts lazy DataFusion plans into record-batch streams.
//!
//! [`read_parquet_df`] creates a minimal DataFusion context for local Parquet input without
//! eagerly collecting rows. [`execute_partitioned`] preserves physical partition streams for
//! throughput-oriented consumers, while [`execute_stream`] exposes one logical stream when
//! downstream file ordering must follow the DataFusion plan.
//!
//! The main spatial job usually builds its own configured session through `engine::session`.
//! These helpers primarily support format adapters that need a compact read-and-stream boundary.

use anyhow::{Context, Result};
use datafusion::dataframe::DataFrame;
use datafusion::datasource::file_format::options::ParquetReadOptions;
use datafusion::execution::context::SessionContext;
use datafusion::physical_plan::SendableRecordBatchStream;
use datafusion_execution::config::SessionConfig;

/// Build a DataFrame over a local Parquet file or directory.
pub async fn read_parquet_df(input_path: &str) -> Result<DataFrame> {
  let session_config = SessionConfig::new().with_collect_statistics(false);
  let ctx = SessionContext::new_with_config(session_config);
  ctx
    .read_parquet(input_path, ParquetReadOptions::default())
    .await
    .context("read parquet")
}

/// Execute every physical partition as an independent record-batch stream.
pub async fn execute_partitioned(df: DataFrame) -> Result<Vec<SendableRecordBatchStream>> {
  df.execute_stream_partitioned()
    .await
    .context("execute partitioned")
}

/// Execute a DataFrame as one logically ordered record-batch stream.
pub async fn execute_stream(df: DataFrame) -> Result<SendableRecordBatchStream> {
  df.execute_stream().await.context("execute stream")
}

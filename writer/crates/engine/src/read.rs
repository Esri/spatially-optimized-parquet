use anyhow::{Context, Result};
use datafusion::dataframe::DataFrame;
use datafusion::datasource::file_format::options::ParquetReadOptions;
use datafusion::execution::context::SessionContext;
use datafusion::physical_plan::SendableRecordBatchStream;
use datafusion_execution::config::SessionConfig;

pub async fn read_parquet_df(input_path: &str) -> Result<DataFrame> {
  let session_config = SessionConfig::new().with_collect_statistics(false);
  let ctx = SessionContext::new_with_config(session_config);
  ctx
    .read_parquet(input_path, ParquetReadOptions::default())
    .await
    .context("read parquet")
}

pub async fn execute_partitioned(df: DataFrame) -> Result<Vec<SendableRecordBatchStream>> {
  df.execute_stream_partitioned()
    .await
    .context("execute partitioned")
}

pub async fn execute_stream(df: DataFrame) -> Result<SendableRecordBatchStream> {
  df.execute_stream().await.context("execute stream")
}

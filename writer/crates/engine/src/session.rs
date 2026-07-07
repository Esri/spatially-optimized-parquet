use std::sync::Arc;

use anyhow::Result;
use datafusion::execution::context::SessionContext;
use datafusion_execution::config::SessionConfig;
use datafusion_execution::runtime_env::{RuntimeEnv, RuntimeEnvBuilder};
use tempfile::TempDir;

const DEFAULT_MEMORY_LIMIT_BYTES: usize = 40 * 1024 * 1024 * 1024;
const DEFAULT_SORT_SPILL_RESERVATION_BYTES: usize = 256 * 1024 * 1024;
const NO_IN_PLACE_SORT_THRESHOLD_BYTES: usize = 1;
const SORT_TARGET_PARTITIONS: usize = 2;
const MEMORY_LIMIT_ENV: &str = "OPT_PARQUET_DF_MEMORY_LIMIT_BYTES";
const SORT_SPILL_RESERVATION_ENV: &str = "OPT_PARQUET_DF_SORT_SPILL_RESERVATION_BYTES";
const TARGET_PARTITIONS_ENV: &str = "OPT_PARQUET_DF_TARGET_PARTITIONS";

pub struct DataFusionSession {
  ctx: SessionContext,
  _spill_dir: TempDir,
}

impl DataFusionSession {
  pub fn context(&self) -> &SessionContext {
    &self.ctx
  }
}

pub fn new_datafusion_session() -> Result<DataFusionSession> {
  let spill_dir = tempfile::Builder::new()
    .prefix("opt-parquet-datafusion-spill-")
    .tempdir()?;
  let runtime = Arc::new(build_runtime_env(spill_dir.path())?);
  let session_config = SessionConfig::new()
    .with_collect_statistics(false)
    .with_target_partitions(configured_target_partitions())
    .with_repartition_file_scans(false)
    .with_repartition_sorts(true)
    .with_prefer_existing_sort(true)
    .with_sort_spill_reservation_bytes(configured_sort_spill_reservation_bytes())
    .with_sort_in_place_threshold_bytes(NO_IN_PLACE_SORT_THRESHOLD_BYTES);
  let ctx = SessionContext::new_with_config_rt(session_config, runtime);
  Ok(DataFusionSession {
    ctx,
    _spill_dir: spill_dir,
  })
}

fn build_runtime_env(spill_dir: &std::path::Path) -> Result<RuntimeEnv> {
  Ok(
    RuntimeEnvBuilder::new()
      .with_memory_limit(configured_memory_limit_bytes(), 1.0)
      .with_temp_file_path(spill_dir)
      .build()?,
  )
}

fn configured_memory_limit_bytes() -> usize {
  env_usize(MEMORY_LIMIT_ENV).unwrap_or(DEFAULT_MEMORY_LIMIT_BYTES)
}

fn configured_sort_spill_reservation_bytes() -> usize {
  env_usize(SORT_SPILL_RESERVATION_ENV).unwrap_or(DEFAULT_SORT_SPILL_RESERVATION_BYTES)
}

pub fn configured_target_partitions() -> usize {
  env_usize(TARGET_PARTITIONS_ENV).unwrap_or(SORT_TARGET_PARTITIONS)
}

fn env_usize(name: &str) -> Option<usize> {
  std::env::var(name)
    .ok()
    .and_then(|value| value.parse::<usize>().ok())
    .filter(|value| *value > 0)
}

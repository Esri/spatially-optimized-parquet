//! Creates the DataFusion execution environment used by analysis, sorting, and Parquet output.
//!
//! [`DataFusionSession::new`] creates a session-scoped spill directory, applies a bounded
//! memory pool, enables sort repartitioning and disk spilling, and preserves existing sort
//! order where possible. File-scan repartitioning stays disabled because input providers
//! either expose their own partitions or rely on DataFusion's native Parquet planning.
//!
//! Memory limits and target partition counts come from the pipeline request. Omitted memory limits
//! use half of total physical memory, while omitted target partition counts retain DataFusion's
//! available-core default. [`DataFusionSession`] owns the temporary directory so spill files cannot
//! disappear while a physical plan still references them.

use std::sync::Arc;

use anyhow::{Context, Result, bail};
use datafusion::execution::context::SessionContext;
use datafusion_execution::config::SessionConfig;
use datafusion_execution::runtime_env::{RuntimeEnv, RuntimeEnvBuilder};
use sysinfo::System;
use tempfile::TempDir;

const DEFAULT_SORT_SPILL_RESERVATION_BYTES: usize = 256 * 1024 * 1024;
const NO_IN_PLACE_SORT_THRESHOLD_BYTES: usize = 1;
const SORT_SPILL_RESERVATION_ENV: &str = "OPT_PARQUET_DF_SORT_SPILL_RESERVATION_BYTES";

/// Owns a DataFusion context and the temporary spill directory required by its runtime.
///
/// Keeping the directory in this value preserves spill files for the full session lifetime.
pub(crate) struct DataFusionSession {
  ctx: SessionContext,
  _spill_dir: TempDir,
}

impl DataFusionSession {
  /// Create the DataFusion session used by spatial analysis and output execution.
  ///
  /// File-scan repartitioning stays disabled because input providers define their own
  /// partition behavior, while sort repartitioning and disk spilling remain enabled.
  pub(crate) fn new(
    memory_limit_bytes: Option<usize>,
    target_partitions: Option<usize>,
  ) -> Result<Self> {
    if memory_limit_bytes == Some(0) {
      bail!("DataFusion memory limit must be >= 1 byte");
    }
    if target_partitions == Some(0) {
      bail!("DataFusion target partitions must be >= 1");
    }
    let spill_dir = tempfile::Builder::new()
      .prefix("opt-parquet-datafusion-spill-")
      .tempdir()?;
    let memory_limit_bytes = memory_limit_bytes.unwrap_or(default_memory_limit_bytes()?);
    let runtime = Arc::new(new_runtime_env(spill_dir.path(), memory_limit_bytes)?);
    let mut session_config = SessionConfig::new()
      .with_collect_statistics(false)
      .with_repartition_file_scans(false)
      .with_repartition_sorts(true)
      .with_prefer_existing_sort(true)
      .with_sort_spill_reservation_bytes(configured_sort_spill_reservation_bytes())
      .with_sort_in_place_threshold_bytes(NO_IN_PLACE_SORT_THRESHOLD_BYTES);
    if let Some(target_partitions) = target_partitions {
      session_config = session_config.with_target_partitions(target_partitions);
    }
    let ctx = SessionContext::new_with_config_rt(session_config, runtime);
    Ok(Self {
      ctx,
      _spill_dir: spill_dir,
    })
  }

  /// Return the configured DataFusion context.
  pub(crate) fn context(&self) -> &SessionContext {
    &self.ctx
  }
}

fn new_runtime_env(spill_dir: &std::path::Path, memory_limit_bytes: usize) -> Result<RuntimeEnv> {
  Ok(
    RuntimeEnvBuilder::new()
      .with_memory_limit(memory_limit_bytes, 1.0)
      .with_temp_file_path(spill_dir)
      .build()?,
  )
}

fn default_memory_limit_bytes() -> Result<usize> {
  let mut system = System::new();
  system.refresh_memory();
  half_physical_memory(system.total_memory())
}

fn half_physical_memory(total_memory_bytes: u64) -> Result<usize> {
  let memory_limit_bytes = total_memory_bytes / 2;
  if memory_limit_bytes == 0 {
    bail!("unable to determine total physical memory");
  }
  usize::try_from(memory_limit_bytes)
    .context("half of total physical memory exceeds this platform's address space")
}

fn configured_sort_spill_reservation_bytes() -> usize {
  env_usize(SORT_SPILL_RESERVATION_ENV).unwrap_or(DEFAULT_SORT_SPILL_RESERVATION_BYTES)
}

fn env_usize(name: &str) -> Option<usize> {
  std::env::var(name)
    .ok()
    .and_then(|value| value.parse::<usize>().ok())
    .filter(|value| *value > 0)
}

#[cfg(test)]
mod tests {
  use datafusion_execution::config::SessionConfig;

  use super::{DataFusionSession, half_physical_memory};

  #[test]
  fn session_owns_spill_directory_for_its_lifetime() {
    let session = DataFusionSession::new(None, None).unwrap();
    let spill_path = session._spill_dir.path().to_path_buf();

    assert!(spill_path.is_dir());
    drop(session);
    assert!(!spill_path.exists());
  }

  #[test]
  fn session_uses_datafusion_target_partition_default() {
    let session = DataFusionSession::new(Some(1024 * 1024), None).unwrap();

    assert_eq!(
      session.context().copied_config().target_partitions(),
      SessionConfig::new().target_partitions()
    );
  }

  #[test]
  fn session_accepts_requested_target_partitions() {
    let session = DataFusionSession::new(Some(1024 * 1024), Some(3)).unwrap();

    assert_eq!(session.context().copied_config().target_partitions(), 3);
  }

  #[test]
  fn default_memory_uses_half_of_physical_memory() {
    assert_eq!(half_physical_memory(16 * 1024).unwrap(), 8 * 1024);
    assert!(half_physical_memory(1).is_err());
  }
}

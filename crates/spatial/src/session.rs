// Copyright 2026 Esri
//
// Licensed under the Apache License Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

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

use std::num::NonZeroUsize;
use std::sync::Arc;

use datafusion::execution::context::SessionContext;
use datafusion_execution::config::SessionConfig;
use datafusion_execution::memory_pool::{FairSpillPool, TrackConsumersPool};
use datafusion_execution::runtime_env::{RuntimeEnv, RuntimeEnvBuilder};
use sysinfo::System;
use tempfile::TempDir;

const DEFAULT_SORT_SPILL_RESERVATION_BYTES: usize = 10 * 1024 * 1024;
const TRACKED_MEMORY_CONSUMER_COUNT: usize = 5;
const NO_IN_PLACE_SORT_THRESHOLD_BYTES: usize = 1;
const SORT_SPILL_RESERVATION_ENV: &str = "OPT_PARQUET_DF_SORT_SPILL_RESERVATION_BYTES";

/// Represents failures while configuring one DataFusion execution session.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
  /// Reports an invalid requested memory limit.
  #[error("DataFusion memory limit must be >= 1 byte")]
  InvalidMemoryLimit,
  /// Reports an invalid requested partition count.
  #[error("DataFusion target partitions must be >= 1")]
  InvalidPartitionCount,
  /// Reports temporary spill-directory creation failures.
  #[error("create DataFusion spill directory: {0}")]
  SpillDirectory(#[source] std::io::Error),
  /// Reports DataFusion runtime construction failures.
  #[error("create DataFusion runtime: {0}")]
  Runtime(#[source] datafusion::common::DataFusionError),
  /// Reports unavailable or unrepresentable system memory.
  #[error("{0}")]
  Memory(String),
}

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
  ) -> Result<Self, SessionError> {
    if memory_limit_bytes == Some(0) {
      return Err(SessionError::InvalidMemoryLimit);
    }
    if target_partitions == Some(0) {
      return Err(SessionError::InvalidPartitionCount);
    }
    let spill_dir = tempfile::Builder::new()
      .prefix("opt-parquet-datafusion-spill-")
      .tempdir()
      .map_err(SessionError::SpillDirectory)?;
    let memory_limit_bytes = memory_limit_bytes.unwrap_or(Self::default_memory_limit_bytes()?);
    let runtime = Arc::new(Self::new_runtime_env(spill_dir.path(), memory_limit_bytes)?);
    let mut session_config = SessionConfig::new()
      .with_collect_statistics(false)
      .with_repartition_file_scans(false)
      .with_repartition_sorts(true)
      .with_prefer_existing_sort(true)
      .with_sort_spill_reservation_bytes(Self::configured_sort_spill_reservation_bytes())
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

  fn new_runtime_env(
    spill_dir: &std::path::Path,
    memory_limit_bytes: usize,
  ) -> Result<RuntimeEnv, SessionError> {
    let memory_pool = Arc::new(TrackConsumersPool::new(
      FairSpillPool::new(memory_limit_bytes),
      NonZeroUsize::new(TRACKED_MEMORY_CONSUMER_COUNT)
        .expect("tracked memory consumer count must be non-zero"),
    ));
    Ok(
      RuntimeEnvBuilder::new()
        .with_memory_pool(memory_pool)
        .with_temp_file_path(spill_dir)
        .build()
        .map_err(SessionError::Runtime)?,
    )
  }

  fn default_memory_limit_bytes() -> Result<usize, SessionError> {
    let mut system = System::new();
    system.refresh_memory();
    Self::half_physical_memory(system.total_memory())
  }

  fn half_physical_memory(total_memory_bytes: u64) -> Result<usize, SessionError> {
    let memory_limit_bytes = total_memory_bytes / 2;
    if memory_limit_bytes == 0 {
      return Err(SessionError::Memory(
        "unable to determine total physical memory".to_string(),
      ));
    }
    usize::try_from(memory_limit_bytes).map_err(|_| {
      SessionError::Memory(
        "half of total physical memory exceeds this platform's address space".to_string(),
      )
    })
  }

  fn configured_sort_spill_reservation_bytes() -> usize {
    Self::env_usize(SORT_SPILL_RESERVATION_ENV).unwrap_or(DEFAULT_SORT_SPILL_RESERVATION_BYTES)
  }

  fn env_usize(name: &str) -> Option<usize> {
    std::env::var(name)
      .ok()
      .and_then(|value| value.parse::<usize>().ok())
      .filter(|value| *value > 0)
  }
}

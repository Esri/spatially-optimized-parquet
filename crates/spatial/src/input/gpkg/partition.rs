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

//! Partitions GeoPackage layer scans into independently executable DataFusion streams.
//!
//! Resolves SQLite rowid boundaries for each requested row range, then falls back to one
//! contiguous scan when boundary discovery cannot preserve that partitioning.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow_schema::SchemaRef;
use datafusion::execution::context::TaskContext;
use datafusion::physical_plan::SendableRecordBatchStream;
use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
use datafusion::physical_plan::streaming::PartitionStream;
use futures_util::TryStreamExt;
use futures_util::stream;
use gdal::Dataset;
use gdal::vector::{LayerAccess, sql};

use crate::input::{InputError, RowRange};

use super::batch_reader::GpkgBatchReader;
use super::open::open_gpkg_dataset;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Defines an inclusive lower and exclusive upper rowid range for one scan partition.
pub(super) struct GpkgScanPartition {
  lower_rowid: Option<i64>,
  upper_rowid: Option<i64>,
}

impl GpkgScanPartition {
  pub(super) fn attribute_filter(&self) -> Option<String> {
    match (self.lower_rowid, self.upper_rowid) {
      (None, None) => None,
      (Some(lower_rowid), None) => Some(format!("rowid >= {lower_rowid}")),
      (None, Some(upper_rowid)) => Some(format!("rowid < {upper_rowid}")),
      (Some(lower_rowid), Some(upper_rowid)) => {
        Some(format!("rowid >= {lower_rowid} AND rowid < {upper_rowid}"))
      }
    }
  }
}

#[derive(Debug)]
/// Opens and streams one independently executable GeoPackage rowid partition.
pub(super) struct GpkgPartitionStream {
  input_path: PathBuf,
  layer_name: String,
  schema: SchemaRef,
  attribute_filter: Option<String>,
}

impl GpkgPartitionStream {
  pub(super) fn new(
    input_path: PathBuf,
    layer_name: String,
    schema: SchemaRef,
    attribute_filter: Option<String>,
  ) -> Self {
    Self {
      input_path,
      layer_name,
      schema,
      attribute_filter,
    }
  }
}

impl PartitionStream for GpkgPartitionStream {
  fn schema(&self) -> &SchemaRef {
    &self.schema
  }

  fn execute(&self, _ctx: Arc<TaskContext>) -> SendableRecordBatchStream {
    let result = GpkgBatchReader::open(
      &self.input_path,
      &self.layer_name,
      self.schema.clone(),
      self.attribute_filter.as_deref(),
      None,
    );

    match result {
      Ok(reader) => Box::pin(RecordBatchStreamAdapter::new(
        self.schema.clone(),
        reader
          .into_stream()
          .map_err(GpkgBatchReader::to_datafusion_error),
      )),
      Err(err) => Box::pin(RecordBatchStreamAdapter::new(
        self.schema.clone(),
        stream::once(async { Err(GpkgBatchReader::to_datafusion_error(err)) }),
      )),
    }
  }
}

impl GpkgScanPartition {
  /// Resolve rowid boundaries that divide a requested row range across DataFusion partitions.
  ///
  /// Boundary discovery issues SQLite offset queries before execution. If any boundary
  /// cannot be resolved, the planner falls back to one contiguous partition.
  pub(super) fn plan(
    path: &Path,
    layer_name: &str,
    total_rows: u64,
    row_range: RowRange,
    target_partitions: usize,
  ) -> Result<Vec<Self>, InputError> {
    let effective_rows = row_range.effective_rows(total_rows);
    let partition_count = Self::effective_count(total_rows, row_range, target_partitions);
    let dataset = open_gpkg_dataset(path)?;
    if partition_count <= 1 {
      return Self::fallback(&dataset, layer_name, total_rows, row_range);
    }

    let mut partitions = Vec::with_capacity(partition_count);
    let start = row_range.start() as u64;
    let end = start + effective_rows;
    let mut lower_rowid = Self::rowid_at_offset(&dataset, layer_name, total_rows, start)?;
    for index in 1..partition_count {
      let offset = start + index as u64 * effective_rows / partition_count as u64;
      let Some(upper_rowid) = Self::query_rowid_at_offset(&dataset, layer_name, offset)? else {
        return Self::fallback(&dataset, layer_name, total_rows, row_range);
      };
      partitions.push(Self {
        lower_rowid,
        upper_rowid: Some(upper_rowid),
      });
      lower_rowid = Some(upper_rowid);
    }

    partitions.push(Self {
      lower_rowid,
      upper_rowid: Self::rowid_at_offset(&dataset, layer_name, total_rows, end)?,
    });
    Ok(partitions)
  }

  fn effective_count(total_rows: u64, row_range: RowRange, target_partitions: usize) -> usize {
    let effective_rows = row_range.effective_rows(total_rows);
    if effective_rows <= 1 {
      return 1;
    }
    target_partitions.max(1).min(effective_rows as usize)
  }

  fn quoted_sqlite_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
  }

  /// Query the rowid at one logical feature offset for partition-boundary discovery.
  fn query_rowid_at_offset(
    dataset: &Dataset,
    layer_name: &str,
    offset: u64,
  ) -> Result<Option<i64>, InputError> {
    let query = format!(
      "SELECT CAST(rowid AS BIGINT) AS partition_rowid FROM {} ORDER BY rowid LIMIT 1 OFFSET {offset}",
      Self::quoted_sqlite_identifier(layer_name)
    );
    let Some(mut result_set) = dataset
      .execute_sql(&query, None, sql::Dialect::SQLITE)
      .map_err(|source| InputError::GeoPackage {
        operation: "query GeoPackage partition rowid",
        source,
      })?
    else {
      return Ok(None);
    };
    let field_index = result_set
      .defn()
      .field_index("partition_rowid")
      .map_err(|source| InputError::GeoPackage {
        operation: "resolve GeoPackage partition rowid field",
        source,
      })?;
    let Some(feature) = result_set.features().next() else {
      return Ok(None);
    };
    feature
      .field_as_integer64(field_index)
      .map_err(|source| InputError::GeoPackage {
        operation: "read GeoPackage partition rowid",
        source,
      })
  }

  fn rowid_at_offset(
    dataset: &Dataset,
    layer_name: &str,
    total_rows: u64,
    offset: u64,
  ) -> Result<Option<i64>, InputError> {
    if offset < total_rows {
      Self::query_rowid_at_offset(dataset, layer_name, offset)
    } else {
      Ok(None)
    }
  }

  fn fallback(
    dataset: &Dataset,
    layer_name: &str,
    total_rows: u64,
    row_range: RowRange,
  ) -> Result<Vec<Self>, InputError> {
    let start = row_range.start() as u64;
    let end = start + row_range.effective_rows(total_rows);
    Ok(vec![Self {
      lower_rowid: Self::rowid_at_offset(dataset, layer_name, total_rows, start)?,
      upper_rowid: Self::rowid_at_offset(dataset, layer_name, total_rows, end)?,
    }])
  }
}

#[cfg(test)]
mod tests {
  use super::GpkgScanPartition;

  #[test]
  fn partition_filter_uses_inclusive_lower_and_exclusive_upper_bounds() {
    let partition = GpkgScanPartition {
      lower_rowid: Some(10),
      upper_rowid: Some(20),
    };

    assert_eq!(
      partition.attribute_filter().as_deref(),
      Some("rowid >= 10 AND rowid < 20")
    );
  }

  #[test]
  fn sqlite_layer_identifier_escapes_embedded_quotes() {
    assert_eq!(
      GpkgScanPartition::quoted_sqlite_identifier("owner\"layer"),
      "\"owner\"\"layer\""
    );
  }
}

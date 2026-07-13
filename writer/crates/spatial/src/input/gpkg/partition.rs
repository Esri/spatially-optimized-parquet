use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use arrow_schema::SchemaRef;
use datafusion::execution::context::TaskContext;
use datafusion::physical_plan::SendableRecordBatchStream;
use datafusion::physical_plan::stream::RecordBatchStreamAdapter;
use datafusion::physical_plan::streaming::PartitionStream;
use futures_util::TryStreamExt;
use futures_util::stream;
use gdal::Dataset;
use gdal::vector::{LayerAccess, sql};

use crate::input::RowRange;

use super::arrow::{gpkg_batch_stream, open_gpkg_batch_state, to_datafusion_error};
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
    let result = open_gpkg_batch_state(
      &self.input_path,
      &self.layer_name,
      self.schema.clone(),
      self.attribute_filter.as_deref(),
      None,
    )
    .with_context(|| format!("failed to stream GeoPackage layer {}", self.layer_name));

    match result {
      Ok(state) => Box::pin(RecordBatchStreamAdapter::new(
        self.schema.clone(),
        gpkg_batch_stream(state).map_err(to_datafusion_error),
      )),
      Err(err) => Box::pin(RecordBatchStreamAdapter::new(
        self.schema.clone(),
        stream::once(async { Err(to_datafusion_error(err)) }),
      )),
    }
  }
}

fn effective_gpkg_scan_partition_count(
  total_rows: u64,
  row_range: RowRange,
  target_partitions: usize,
) -> usize {
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
fn query_layer_rowid_at_offset(
  dataset: &Dataset,
  layer_name: &str,
  offset: u64,
) -> Result<Option<i64>> {
  let query = format!(
    "SELECT CAST(rowid AS BIGINT) AS partition_rowid FROM {} ORDER BY rowid LIMIT 1 OFFSET {offset}",
    quoted_sqlite_identifier(layer_name)
  );
  let Some(mut result_set) = dataset.execute_sql(&query, None, sql::Dialect::SQLITE)? else {
    return Ok(None);
  };
  let field_index = result_set.defn().field_index("partition_rowid")?;
  let Some(feature) = result_set.features().next() else {
    return Ok(None);
  };
  feature.field_as_integer64(field_index).map_err(Into::into)
}

fn rowid_at_offset(
  dataset: &Dataset,
  layer_name: &str,
  total_rows: u64,
  offset: u64,
) -> Result<Option<i64>> {
  if offset < total_rows {
    query_layer_rowid_at_offset(dataset, layer_name, offset)
  } else {
    Ok(None)
  }
}

fn fallback_gpkg_scan_partitions(
  dataset: &Dataset,
  layer_name: &str,
  total_rows: u64,
  row_range: RowRange,
) -> Result<Vec<GpkgScanPartition>> {
  let start = row_range.start as u64;
  let end = start + row_range.effective_rows(total_rows);
  Ok(vec![GpkgScanPartition {
    lower_rowid: rowid_at_offset(dataset, layer_name, total_rows, start)?,
    upper_rowid: rowid_at_offset(dataset, layer_name, total_rows, end)?,
  }])
}

/// Resolve rowid boundaries that divide a requested row range across DataFusion partitions.
///
/// Boundary discovery issues SQLite offset queries before execution. If any boundary
/// cannot be resolved, the planner falls back to one contiguous partition.
pub(super) fn plan_gpkg_scan_partitions(
  path: &Path,
  layer_name: &str,
  total_rows: u64,
  row_range: RowRange,
  target_partitions: usize,
) -> Result<Vec<GpkgScanPartition>> {
  let effective_rows = row_range.effective_rows(total_rows);
  let partition_count =
    effective_gpkg_scan_partition_count(total_rows, row_range, target_partitions);
  let dataset = open_gpkg_dataset(path)?;
  if partition_count <= 1 {
    return fallback_gpkg_scan_partitions(&dataset, layer_name, total_rows, row_range);
  }

  let mut partitions = Vec::with_capacity(partition_count);
  let start = row_range.start as u64;
  let end = start + effective_rows;
  let mut lower_rowid = rowid_at_offset(&dataset, layer_name, total_rows, start)?;
  for index in 1..partition_count {
    let offset = start + index as u64 * effective_rows / partition_count as u64;
    let Some(upper_rowid) = query_layer_rowid_at_offset(&dataset, layer_name, offset)? else {
      return fallback_gpkg_scan_partitions(&dataset, layer_name, total_rows, row_range);
    };
    partitions.push(GpkgScanPartition {
      lower_rowid,
      upper_rowid: Some(upper_rowid),
    });
    lower_rowid = Some(upper_rowid);
  }

  partitions.push(GpkgScanPartition {
    lower_rowid,
    upper_rowid: rowid_at_offset(&dataset, layer_name, total_rows, end)?,
  });
  Ok(partitions)
}

#[cfg(test)]
mod tests {
  use super::{GpkgScanPartition, effective_gpkg_scan_partition_count, quoted_sqlite_identifier};
  use crate::input::RowRange;

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
  fn partition_count_does_not_exceed_requested_rows() {
    assert_eq!(
      effective_gpkg_scan_partition_count(
        100,
        RowRange {
          start: 10,
          num: Some(2),
        },
        8,
      ),
      2
    );
  }

  #[test]
  fn partition_count_uses_one_partition_for_empty_or_single_row_ranges() {
    assert_eq!(
      effective_gpkg_scan_partition_count(
        100,
        RowRange {
          start: 100,
          num: None,
        },
        8,
      ),
      1
    );
    assert_eq!(
      effective_gpkg_scan_partition_count(
        100,
        RowRange {
          start: 10,
          num: Some(1),
        },
        8,
      ),
      1
    );
  }

  #[test]
  fn partition_count_clamps_zero_target_to_one() {
    assert_eq!(
      effective_gpkg_scan_partition_count(100, RowRange::default(), 0),
      1
    );
  }

  #[test]
  fn sqlite_layer_identifier_escapes_embedded_quotes() {
    assert_eq!(
      quoted_sqlite_identifier("owner\"layer"),
      "\"owner\"\"layer\""
    );
  }
}

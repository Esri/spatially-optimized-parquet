//! Persists range-partitioned optimized output through custom physical sink planning.

use anyhow::Result;
use datafusion::dataframe::DataFrame;

use crate::optimized::ResolvedOptimization;
use crate::optimized::clustering::ClusterRangeBoundaries;
use crate::output::{OutputPath, ParquetOutputWriter, ParquetWriterOptions};
use crate::pipeline::{PipelineWarningStore, SharedWriteReporter};

use super::dataframe;
use super::sort::PartitionedSortConfig;

/// Write range-partitioned optimized GeoParquet files.
pub(crate) async fn write(
  input_dataframe: DataFrame,
  output_path: &OutputPath,
  source_schema: &arrow_schema::Schema,
  optimization: &ResolvedOptimization,
  covering: bool,
  compression: Option<&str>,
  total_input_rows: u64,
  write_reporter: Option<SharedWriteReporter>,
  warning_store: PipelineWarningStore,
) -> Result<u64> {
  let clustering_family = optimization.geometry().clustering_family;
  let partition_column = clustering_family.cluster_partition_column();
  clustering_family.validate_partition_column(source_schema)?;
  let range_source = dataframe::range_source(input_dataframe.clone(), optimization)?;
  let boundaries = ClusterRangeBoundaries::compute(
    range_source,
    clustering_family.cluster_key_column(),
    output_path.part_count(),
  )
  .await?;
  let dataframe = dataframe::dataframe(
    input_dataframe,
    source_schema,
    optimization,
    &boundaries,
    covering,
    warning_store,
  )?;
  let metadata = optimization.parquet_metadata(covering)?;
  let writer_options = ParquetWriterOptions::new(compression.unwrap_or("snappy"), &metadata)?
    .with_delta_binary_packed_columns(optimization.delta_binary_packed_column_paths())
    .into_datafusion();
  let partitioned_sort = PartitionedSortConfig::new(
    partition_column,
    clustering_family.cluster_key_column(),
    output_path.part_count(),
    true,
  );
  ParquetOutputWriter::new(total_input_rows, write_reporter)
    .write_partitioned(
      dataframe,
      output_path.path().to_string_lossy().into_owned(),
      vec![partition_column.to_string()],
      writer_options,
      |input| partitioned_sort.insert_into(input),
    )
    .await
}

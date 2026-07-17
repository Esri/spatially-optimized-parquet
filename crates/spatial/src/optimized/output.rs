//! Coordinates resolved optimization state and physical output execution.

use anyhow::{Context, Result};
use arrow_schema::Schema;
use datafusion::dataframe::DataFrame;

use crate::geoparquet::{PreparedSpatialFrame, resolve_source};
use crate::input::{InputSource, RowRange};
use crate::optimized::clustering::{
  cluster_key_column, cluster_partition_column, validate_cluster_partition_column,
};
use crate::optimized::extent::TargetExtentResolver;
use crate::optimized::metadata::parquet_metadata;
use crate::optimized::multiscale::create_multiscale_level_specs;
use crate::optimized::projection::{
  partitioned_projection, partitioned_range_source, single_file_projection,
};
use crate::optimized::range_boundaries::compute_cluster_range_boundaries;
use crate::optimized::write::{PartitionedOutputWriter, write_optimized_single_file};
use crate::optimized::{ClusteringFamily, OptimizedGeometry, ResolvedOptimization};
use crate::output::{OutputLayout, ReprojectionSpec};
use crate::pipeline::{OutputExecutionOptions, PipelineWarningStore, SharedWriteReporter};

/// Coordinates optimized resolution and output behind one crate-private product boundary.
pub(crate) struct OptimizedOutput<'a, State> {
  input_dataframe: DataFrame,
  output_layout: &'a OutputLayout,
  source_schema: &'a Schema,
  total_input_rows: u64,
  write_reporter: Option<SharedWriteReporter>,
  warning_store: PipelineWarningStore,
  state: State,
}

/// Stores unresolved input selection state during optimized output analysis.
pub(crate) struct PendingOutputState<'a> {
  input: &'a dyn InputSource,
  row_range: RowRange,
}

/// Stores resolved optimization and writer policy for optimized output execution.
pub(crate) struct ResolvedOutputState {
  covering: bool,
  compression: Option<String>,
  optimization: ResolvedOptimization,
}

impl<'a> OptimizedOutput<'a, PendingOutputState<'a>> {
  /// Construct optimized output resolution for one prepared input selection.
  pub(crate) fn new(
    input: &'a dyn InputSource,
    input_dataframe: DataFrame,
    output_layout: &'a OutputLayout,
    source_schema: &'a Schema,
    total_input_rows: u64,
    row_range: RowRange,
    write_reporter: Option<SharedWriteReporter>,
    warning_store: PipelineWarningStore,
  ) -> Self {
    Self {
      input_dataframe,
      output_layout,
      source_schema,
      total_input_rows,
      write_reporter,
      warning_store,
      state: PendingOutputState { input, row_range },
    }
  }

  /// Resolve all optimized state for one prepared spatial pipeline.
  pub(crate) async fn resolve(
    self,
    options: &OutputExecutionOptions,
  ) -> Result<OptimizedOutput<'a, ResolvedOutputState>> {
    let mut source = resolve_source(
      self.state.input,
      self.input_dataframe.clone(),
      self.source_schema,
      options.geometry_column.as_deref(),
      options.input_wkid,
      self.state.row_range,
    )
    .await?;
    source.strip_dimensions(options.strip_z, options.strip_m);
    let geometry = OptimizedGeometry::resolve(&source)?;
    let source_projjson = source
      .source_spatial_reference
      .projjson
      .as_ref()
      .context("missing resolved source CRS PROJJSON")?;
    let reprojection =
      ReprojectionSpec::from_source_projjson(source_projjson, options.output_wkid)?;
    let prepared = PreparedSpatialFrame::new(
      self.input_dataframe.clone(),
      self.source_schema,
      &source,
      &reprojection,
      options.strip_z,
      options.strip_m,
    )?;
    let target_extent = TargetExtentResolver::new(self.state.input, self.state.row_range)
      .resolve(&source, &prepared, &reprojection)
      .await?;
    let encodings = match geometry.clustering_family {
      ClusteringFamily::PointGeometry => Vec::new(),
      ClusteringFamily::ComplexGeometry => {
        create_multiscale_level_specs(options.output_wkid, geometry.geometry_type)?
      }
    };
    Ok(OptimizedOutput {
      input_dataframe: prepared.dataframe(),
      output_layout: self.output_layout,
      source_schema: self.source_schema,
      total_input_rows: self.total_input_rows,
      write_reporter: self.write_reporter,
      warning_store: self.warning_store,
      state: ResolvedOutputState {
        covering: options.covering,
        compression: options.compression.clone(),
        optimization: ResolvedOptimization::new(
          source.source_metadata,
          geometry,
          reprojection,
          target_extent,
          encodings,
          options.multiscale_encoding,
        ),
      },
    })
  }
}

impl<'a> OptimizedOutput<'a, ResolvedOutputState> {
  /// Write globally sorted optimized rows to one Parquet file.
  pub(crate) async fn write_single_file(&self) -> Result<u64> {
    let dataframe = single_file_projection(
      &self.state.optimization,
      self.input_dataframe.clone(),
      self.source_schema,
      self.state.covering,
      self.warning_store.clone(),
    )?;
    let metadata = parquet_metadata(&self.state.optimization, self.state.covering)?;
    let hidden_sort_column = Some(cluster_key_column(
      self.state.optimization.geometry().clustering_family,
    ));
    write_optimized_single_file(
      dataframe,
      self.output_layout,
      self.state.compression.as_deref(),
      metadata,
      hidden_sort_column,
      self.total_input_rows,
      self.write_reporter.clone(),
      self.state.optimization.delta_binary_packed_column_paths(),
    )
    .await
  }

  /// Write range-partitioned optimized rows through the custom sink.
  pub(crate) async fn write_partitioned(&self) -> Result<u64> {
    validate_cluster_partition_column(
      self.source_schema,
      Some(cluster_partition_column(
        self.state.optimization.geometry().clustering_family,
      )),
    )?;
    let range_source =
      partitioned_range_source(&self.state.optimization, self.input_dataframe.clone())?;
    let boundaries = compute_cluster_range_boundaries(
      range_source,
      cluster_key_column(self.state.optimization.geometry().clustering_family),
      self.output_layout.part_count(),
    )
    .await?;
    let dataframe = partitioned_projection(
      &self.state.optimization,
      self.input_dataframe.clone(),
      self.source_schema,
      &boundaries,
      self.state.covering,
      self.warning_store.clone(),
    )?;
    let metadata = parquet_metadata(&self.state.optimization, self.state.covering)?;
    PartitionedOutputWriter::new(
      self.output_layout,
      self.state.compression.as_deref(),
      &self.state.optimization,
      self.total_input_rows,
      self.write_reporter.clone(),
    )
    .write(dataframe, metadata)
    .await
  }
}

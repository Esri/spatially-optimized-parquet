//! Coordinates resolved optimization state and physical output execution.

use anyhow::{Context, Result};
use arrow_schema::Schema;
use datafusion::dataframe::DataFrame;
use engine::OutputLayout;

use crate::geoparquet::{resolve_source, validate_covering_configuration};
use crate::input::{InputSource, RowRange};
use crate::optimized::clustering::{
  cluster_key_column, cluster_partition_column, validate_cluster_partition_column,
};
use crate::optimized::extent::TargetExtentResolver;
use crate::optimized::metadata::parquet_metadata;
use crate::optimized::multiscale::create_geometry_encodings;
use crate::optimized::projection::{
  partitioned_projection, partitioned_range_source, single_file_projection,
};
use crate::optimized::range_boundaries::compute_cluster_range_boundaries;
use crate::optimized::write::{PartitionedOutputWriter, write_optimized_single_file};
use crate::optimized::{ClusteringFamily, OptimizedGeometry, ResolvedOptimization};
use crate::output::ReprojectionSpec;
use crate::progress::{finish_row_bar, row_bar};

/// Coordinates optimized resolution and output behind one crate-private product boundary.
pub(crate) struct OptimizedOutput<'a, State> {
  input_dataframe: DataFrame,
  output_layout: &'a OutputLayout,
  source_schema: &'a Schema,
  total_input_rows: u64,
  explain: bool,
  state: State,
}

/// Stores unresolved input selection state during optimized output analysis.
pub(crate) struct PendingOutputState<'a> {
  input: &'a dyn InputSource,
  row_range: RowRange,
}

/// Stores resolved optimization and writer policy for optimized output execution.
pub(crate) struct ResolvedOutputState<'a> {
  covering: bool,
  compression: Option<&'a str>,
  progress: bool,
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
    explain: bool,
  ) -> Self {
    Self {
      input_dataframe,
      output_layout,
      source_schema,
      total_input_rows,
      explain,
      state: PendingOutputState { input, row_range },
    }
  }

  /// Resolve all optimized state for one prepared spatial pipeline.
  pub(crate) async fn resolve(
    self,
    geometry_column: Option<&str>,
    input_wkid: Option<u32>,
    output_wkid: u32,
    covering: bool,
    compression: Option<&'a str>,
    progress: bool,
  ) -> Result<OptimizedOutput<'a, ResolvedOutputState<'a>>> {
    validate_covering_configuration(covering, self.source_schema)?;
    let source = resolve_source(
      self.state.input,
      self.input_dataframe.clone(),
      self.source_schema,
      geometry_column,
      input_wkid,
      self.state.row_range,
    )
    .await?;
    let geometry = OptimizedGeometry::resolve(&source)?;
    let source_projjson = source
      .source_spatial_reference
      .projjson
      .as_ref()
      .context("missing resolved source CRS PROJJSON")?;
    let reprojection = ReprojectionSpec::from_source_projjson(source_projjson, output_wkid)?;
    let target_extent = TargetExtentResolver::new(
      self.state.input,
      self.input_dataframe.clone(),
      self.total_input_rows,
      self.state.row_range,
      progress,
      self.explain,
    )
    .resolve(&source, &geometry, &reprojection)
    .await?;
    let encodings = match geometry.clustering_family {
      ClusteringFamily::Point => Vec::new(),
      ClusteringFamily::NonPoint => create_geometry_encodings(output_wkid, geometry.geometry_type)?,
    };
    Ok(OptimizedOutput {
      input_dataframe: self.input_dataframe,
      output_layout: self.output_layout,
      source_schema: self.source_schema,
      total_input_rows: self.total_input_rows,
      explain: self.explain,
      state: ResolvedOutputState {
        covering,
        compression,
        progress,
        optimization: ResolvedOptimization::new(
          source.source_metadata,
          geometry,
          reprojection,
          target_extent,
          encodings,
        ),
      },
    })
  }
}

impl<'a> OptimizedOutput<'a, ResolvedOutputState<'a>> {
  /// Write globally sorted optimized rows to one Parquet file.
  pub(crate) async fn write_single_file(&self) -> Result<u64> {
    let dataframe = single_file_projection(
      &self.state.optimization,
      self.input_dataframe.clone(),
      self.source_schema,
      self.state.covering,
    )?;
    let metadata = parquet_metadata(&self.state.optimization, self.state.covering)?;
    write_optimized_single_file(
      dataframe,
      self.output_layout,
      self.state.compression,
      metadata,
      self.state.progress,
      self.total_input_rows,
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
    let range_bar = row_bar(
      self.state.progress,
      "Computing partition ranges",
      self.total_input_rows,
    );
    let range_source =
      partitioned_range_source(&self.state.optimization, self.input_dataframe.clone())?;
    let boundaries = compute_cluster_range_boundaries(
      range_source,
      cluster_key_column(self.state.optimization.geometry().clustering_family),
      self.output_layout.part_count(),
      &range_bar,
      self.total_input_rows,
      self.explain,
    )
    .await?;
    finish_row_bar(
      &range_bar,
      self.total_input_rows,
      "Computed partition ranges".to_string(),
    );
    let dataframe = partitioned_projection(
      &self.state.optimization,
      self.input_dataframe.clone(),
      self.source_schema,
      &boundaries,
      self.state.covering,
    )?;
    let metadata = parquet_metadata(&self.state.optimization, self.state.covering)?;
    PartitionedOutputWriter::new(
      self.output_layout,
      self.state.compression,
      &self.state.optimization,
      self.state.progress,
      self.total_input_rows,
      self.explain,
    )
    .write(dataframe, metadata)
    .await
  }
}
